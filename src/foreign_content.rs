use educe::Educe;

use crate::{
    functions::is_whitespace,
    html_minifier_helper::{next_comment_state, raw_text_end_tag},
    script_data::ScriptData,
};

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
enum Namespace {
    #[default]
    Html,
    Svg,
    Math,
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
enum Integration {
    #[default]
    None,
    Html,
    MathText,
}

#[derive(Debug, Clone)]
struct Element {
    start:       usize,
    length:      usize,
    namespace:   Namespace,
    integration: Integration,
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
enum Step {
    #[default]
    Data,
    TagOpen,
    EndTagOpen,
    TagName,
    BeforeAttribute,
    AttributeName,
    AfterAttribute,
    BeforeValue,
    QuotedValue,
    UnquotedValue,
    SelfClosing,
    Declaration,
    BogusComment,
    Comment,
    Cdata,
    Raw,
}

#[derive(Debug, Default, Copy, Clone, Eq, PartialEq)]
enum Reference {
    #[default]
    None,
    Start,
    NumberStart,
    Decimal,
    Hexadecimal,
    Named,
}

/// Match the two integration encodings without retaining an attribute value.
#[derive(Debug, Clone)]
struct Encoding {
    candidates: u8,
    position:   u8,
    reference:  Reference,
    number:     u32,
    digits:     bool,
    named:      [u8; 5],
    named_len:  u8,
}

impl Default for Encoding {
    fn default() -> Self {
        Self {
            candidates: 3,
            position:   0,
            reference:  Reference::None,
            number:     0,
            digits:     false,
            named:      [0; 5],
            named_len:  0,
        }
    }
}

impl Encoding {
    fn decoded(&mut self, c: u32) {
        let targets: [&[u8]; 2] = [b"text/html", b"application/xhtml+xml"];

        for (index, target) in targets.iter().enumerate() {
            if !u8::try_from(c).ok().is_some_and(|e| {
                target.get(usize::from(self.position)).is_some_and(|t| e.eq_ignore_ascii_case(t))
            }) {
                self.candidates &= !(1 << index);
            }
        }

        self.position = self.position.saturating_add(1);
    }

    fn end_number(&mut self) {
        if self.digits {
            self.decoded(self.number);
        } else {
            self.candidates = 0;
        }
        self.reference = Reference::None;
    }

    fn feed(&mut self, e: u8) {
        loop {
            if self.candidates == 0 {
                return;
            }

            match self.reference {
                Reference::None => {
                    if e == b'&' {
                        self.reference = Reference::Start;
                    } else {
                        self.decoded(u32::from(e));
                    }
                },
                Reference::Start => {
                    if e == b'#' {
                        self.number = 0;
                        self.digits = false;
                        self.reference = Reference::NumberStart;
                    } else {
                        self.named_len = 0;
                        self.reference = Reference::Named;
                        continue;
                    }
                },
                Reference::NumberStart => {
                    self.reference = if matches!(e, b'x' | b'X') {
                        Reference::Hexadecimal
                    } else {
                        Reference::Decimal
                    };
                    if self.reference == Reference::Decimal {
                        continue;
                    }
                },
                Reference::Decimal | Reference::Hexadecimal => {
                    let radix = if self.reference == Reference::Decimal { 10 } else { 16 };
                    if let Some(digit) = char::from(e).to_digit(radix) {
                        self.number = self.number.saturating_mul(radix).saturating_add(digit);
                        self.digits = true;
                    } else {
                        self.end_number();
                        if e != b';' {
                            continue;
                        }
                    }
                },
                Reference::Named => {
                    let position = usize::from(self.named_len);
                    if position == self.named.len() {
                        self.candidates = 0;
                        return;
                    }
                    self.named[position] = e;
                    self.named_len += 1;
                    if e == b';' {
                        // Only these named references produce characters in either encoding.
                        match &self.named[..usize::from(self.named_len)] {
                            b"sol;" => self.decoded(u32::from(b'/')),
                            b"plus;" => self.decoded(u32::from(b'+')),
                            _ => self.candidates = 0,
                        }
                        self.reference = Reference::None;
                    }
                },
            }
            return;
        }
    }

    fn finish(&mut self) {
        match self.reference {
            Reference::Decimal | Reference::Hexadecimal => self.end_number(),
            Reference::None => (),
            _ => self.candidates = 0,
        }
    }

    fn supported(&self) -> bool {
        (self.candidates & 1 != 0 && self.position == 9)
            || (self.candidates & 2 != 0 && self.position == 21)
    }
}

/// Follow element boundaries while the whole foreign subtree is preserved.
#[derive(Educe, Clone)]
#[educe(Debug, Default)]
pub(crate) struct ForeignContent {
    step:             Step,
    active:           bool,
    plaintext:        bool,
    root:             bool,
    root_namespace:   Namespace,
    elements:         Vec<Element>,
    names:            Vec<u8>,
    tag:              Vec<u8>,
    end_tag:          bool,
    attribute:        [u8; 8],
    attribute_length: u8,
    encoding_seen:    bool,
    collect_encoding: bool,
    encoding:         Encoding,
    quote:            u8,
    counter:          u8,
    declaration:      u8,
    raw_end:          &'static [u8],
    script_data:      ScriptData,
}

impl ForeignContent {
    pub(crate) fn reset(&mut self) {
        self.step = Step::Data;
        self.active = false;
        self.plaintext = false;
        self.root = false;
        self.root_namespace = Namespace::Html;
        self.elements.clear();
        self.names.clear();
        self.tag.clear();
        self.end_tag = false;
        self.attribute_length = 0;
        self.encoding_seen = false;
        self.collect_encoding = false;
        self.encoding = Encoding::default();
        self.quote = 0;
        self.counter = 0;
        self.declaration = 0;
        self.raw_end = b"";
        self.script_data.reset();
    }

    pub(crate) fn start(&mut self, tag: &[u8]) {
        self.reset();
        self.active = true;
        self.root = true;
        self.root_namespace =
            if tag.eq_ignore_ascii_case(b"svg") { Namespace::Svg } else { Namespace::Math };
        self.tag.extend(tag.iter().map(u8::to_ascii_lowercase));
        self.step = Step::TagName;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active
    }

    pub(crate) fn is_plaintext(&self) -> bool {
        self.plaintext
    }

    pub(crate) fn can_finish(&self) -> bool {
        // Open ordinary elements and trailing literal tag prefixes do not require closing tags.
        self.plaintext || matches!(self.step, Step::Data | Step::TagOpen | Step::EndTagOpen)
    }

    fn begin_tag(&mut self, e: u8, end_tag: bool) {
        self.tag.clear();
        self.tag.push(e.to_ascii_lowercase());
        self.end_tag = end_tag;
        self.encoding_seen = false;
        self.collect_encoding = false;
        self.encoding = Encoding::default();
        self.step = Step::TagName;
    }

    fn begin_attribute(&mut self, e: u8) {
        self.attribute[0] = e.to_ascii_lowercase();
        self.attribute_length = 1;
        self.collect_encoding = false;
        self.step = Step::AttributeName;
    }

    fn end_attribute_name(&mut self) {
        self.collect_encoding = !self.end_tag
            && self.tag == b"annotation-xml"
            && !self.encoding_seen
            && self.attribute_length == 8
            && &self.attribute == b"encoding";
        self.encoding_seen |= self.collect_encoding;
    }

    fn end_value(&mut self) {
        if self.collect_encoding {
            self.encoding.finish();
            self.collect_encoding = false;
        }
    }

    fn namespace(&self) -> Namespace {
        if self.root {
            return self.root_namespace;
        }
        let Some(parent) = self.elements.last() else {
            return Namespace::Html;
        };
        let html = parent.namespace == Namespace::Html
            || parent.integration == Integration::Html
            || (parent.integration == Integration::MathText
                && !matches!(self.tag.as_slice(), b"mglyph" | b"malignmark"));

        if html {
            match self.tag.as_slice() {
                b"svg" => Namespace::Svg,
                b"math" => Namespace::Math,
                _ => Namespace::Html,
            }
        } else if parent.namespace == Namespace::Math
            && self.tag == b"svg"
            && &self.names[parent.start..parent.start + parent.length] == b"annotation-xml"
        {
            Namespace::Svg
        } else {
            parent.namespace
        }
    }

    fn finish_tag(&mut self, self_closing: bool) {
        self.end_value();
        self.step = Step::Data;

        if self.end_tag {
            if let Some(index) = self.elements.iter().rposition(|element| {
                self.names[element.start..element.start + element.length] == self.tag
            }) {
                self.names.truncate(self.elements[index].start);
                self.elements.truncate(index);
                self.active = !self.elements.is_empty();
            }
            return;
        }

        let namespace = self.namespace();
        let empty = if namespace == Namespace::Html {
            matches!(
                self.tag.as_slice(),
                b"area"
                    | b"base"
                    | b"br"
                    | b"col"
                    | b"embed"
                    | b"hr"
                    | b"img"
                    | b"input"
                    | b"link"
                    | b"meta"
                    | b"param"
                    | b"source"
                    | b"track"
                    | b"wbr"
            )
        } else {
            self_closing
        };

        if empty {
            if self.root {
                self.active = false;
            }
            self.root = false;
            return;
        }

        let integration = match (namespace, self.tag.as_slice()) {
            (Namespace::Svg, b"foreignobject" | b"desc" | b"title") => Integration::Html,
            (Namespace::Math, b"mi" | b"mo" | b"mn" | b"ms" | b"mtext") => Integration::MathText,
            (Namespace::Math, b"annotation-xml") if self.encoding.supported() => Integration::Html,
            _ => Integration::None,
        };
        self.elements.push(Element {
            start: self.names.len(),
            length: self.tag.len(),
            namespace,
            integration,
        });
        self.names.extend_from_slice(&self.tag);
        self.root = false;

        if namespace == Namespace::Html {
            if self.tag == b"plaintext" {
                self.plaintext = true;
            } else if let Some(end_tag) = raw_text_end_tag(&self.tag) {
                self.raw_end = end_tag;
                self.counter = 0;
                self.script_data.reset();
                self.step = Step::Raw;
            }
        }
    }

    pub(crate) fn feed(&mut self, e: u8) {
        if !self.active || self.plaintext {
            return;
        }

        loop {
            match self.step {
                Step::Data => {
                    if e == b'<' {
                        self.step = Step::TagOpen;
                    }
                },
                Step::TagOpen => match e {
                    b'/' => self.step = Step::EndTagOpen,
                    b'!' => {
                        self.counter = 0;
                        self.declaration = 0;
                        self.step = Step::Declaration;
                    },
                    b'?' => self.step = Step::BogusComment,
                    _ if e.is_ascii_alphabetic() => self.begin_tag(e, false),
                    _ => {
                        self.step = Step::Data;
                        continue;
                    },
                },
                Step::EndTagOpen => match e {
                    b'>' => self.step = Step::Data,
                    _ if e.is_ascii_alphabetic() => self.begin_tag(e, true),
                    _ => self.step = Step::BogusComment,
                },
                Step::TagName => match e {
                    b'>' => self.finish_tag(false),
                    b'/' => self.step = Step::SelfClosing,
                    _ if is_whitespace(e) => self.step = Step::BeforeAttribute,
                    _ => self.tag.push(e.to_ascii_lowercase()),
                },
                Step::BeforeAttribute => match e {
                    b'>' => self.finish_tag(false),
                    b'/' => self.step = Step::SelfClosing,
                    _ if is_whitespace(e) => (),
                    _ => self.begin_attribute(e),
                },
                Step::AttributeName => {
                    if matches!(e, b'=' | b'/' | b'>') || is_whitespace(e) {
                        self.end_attribute_name();
                        if e == b'=' {
                            self.step = Step::BeforeValue;
                        } else {
                            self.step = Step::AfterAttribute;
                            continue;
                        }
                    } else {
                        let position = usize::from(self.attribute_length);
                        if position < self.attribute.len() {
                            self.attribute[position] = e.to_ascii_lowercase();
                        }
                        self.attribute_length = self.attribute_length.saturating_add(1);
                    }
                },
                Step::AfterAttribute => match e {
                    b'=' => self.step = Step::BeforeValue,
                    b'>' => self.finish_tag(false),
                    b'/' => self.step = Step::SelfClosing,
                    _ if is_whitespace(e) => (),
                    _ => self.begin_attribute(e),
                },
                Step::BeforeValue => match e {
                    b'"' | b'\'' => {
                        self.quote = e;
                        self.step = Step::QuotedValue;
                    },
                    b'>' => self.finish_tag(false),
                    _ if is_whitespace(e) => (),
                    _ => {
                        self.step = Step::UnquotedValue;
                        continue;
                    },
                },
                Step::QuotedValue => {
                    if e == self.quote {
                        self.end_value();
                        self.step = Step::BeforeAttribute;
                    } else if self.collect_encoding {
                        self.encoding.feed(e);
                    }
                },
                Step::UnquotedValue => {
                    if e == b'>' {
                        self.finish_tag(false);
                    } else if is_whitespace(e) {
                        self.end_value();
                        self.step = Step::BeforeAttribute;
                    } else if self.collect_encoding {
                        self.encoding.feed(e);
                    }
                },
                Step::SelfClosing => {
                    if e == b'>' {
                        self.finish_tag(true);
                    } else {
                        self.step = Step::BeforeAttribute;
                        continue;
                    }
                },
                Step::Declaration => {
                    if self.counter == 0 {
                        self.declaration = match e {
                            b'-' => 1,
                            b'[' if self
                                .elements
                                .last()
                                .is_some_and(|element| element.namespace != Namespace::Html) =>
                            {
                                2
                            },
                            _ => 0,
                        };
                        self.counter = 1;
                        if self.declaration == 0 {
                            self.step = Step::BogusComment;
                            continue;
                        }
                    } else if self.declaration == 1 && e == b'-' {
                        self.counter = 4;
                        self.step = Step::Comment;
                    } else if self.declaration == 2 && e == b"[CDATA["[usize::from(self.counter)] {
                        self.counter += 1;
                        if self.counter == 7 {
                            self.counter = 0;
                            self.step = Step::Cdata;
                        }
                    } else {
                        self.step = Step::BogusComment;
                        continue;
                    }
                },
                Step::BogusComment => {
                    if e == b'>' {
                        self.step = Step::Data;
                    }
                },
                Step::Comment => match next_comment_state(self.counter, e) {
                    Some(counter) => self.counter = counter,
                    None => self.step = Step::Data,
                },
                Step::Cdata => {
                    if e == b']' {
                        self.counter = (self.counter + 1).min(2);
                    } else {
                        if self.counter == 2 && e == b'>' {
                            self.step = Step::Data;
                        }
                        self.counter = 0;
                    }
                },
                Step::Raw => {
                    if self.script_data.ends(e, self.raw_end, &mut self.counter) {
                        self.tag.clear();
                        self.tag.extend_from_slice(&self.raw_end[2..]);
                        self.raw_end = b"";
                        self.end_tag = true;
                        self.collect_encoding = false;
                        self.step = Step::BeforeAttribute;
                        continue;
                    }
                },
            }
            return;
        }
    }
}
