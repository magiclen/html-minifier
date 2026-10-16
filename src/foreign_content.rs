use educe::Educe;

use crate::{functions::*, script_data::ScriptData};

/// The longest tag name that is collected. Longer names are compared by their first bytes.
const MAX_TAG_LENGTH: usize = 255;

/// The deepest nesting of elements that is followed. The rest of the input after a deeper element is kept as it is.
const MAX_DEPTH: usize = 1024;

/// Check whether an HTML start tag leaves foreign content, such as `<p>` in `<svg>`.
fn is_breakout_tag(tag: &[u8]) -> bool {
    matches!(
        tag,
        b"b" | b"big"
            | b"blockquote"
            | b"body"
            | b"br"
            | b"center"
            | b"code"
            | b"dd"
            | b"div"
            | b"dl"
            | b"dt"
            | b"em"
            | b"embed"
            | b"h1"
            | b"h2"
            | b"h3"
            | b"h4"
            | b"h5"
            | b"h6"
            | b"head"
            | b"hr"
            | b"i"
            | b"img"
            | b"li"
            | b"listing"
            | b"menu"
            | b"meta"
            | b"nobr"
            | b"ol"
            | b"p"
            | b"pre"
            | b"ruby"
            | b"s"
            | b"small"
            | b"span"
            | b"strong"
            | b"strike"
            | b"sub"
            | b"sup"
            | b"table"
            | b"tt"
            | b"u"
            | b"ul"
            | b"var"
    )
}

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

impl Element {
    /// Check whether a start tag in this element follows the rules for HTML content, such as a tag in `<foreignObject>`.
    fn takes_html_tag(&self, tag: &[u8]) -> bool {
        self.namespace == Namespace::Html
            || self.integration == Integration::Html
            || (self.integration == Integration::MathText
                && !matches!(tag, b"mglyph" | b"malignmark"))
    }
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
    #[educe(Debug(method = "str_bytes_fmt"))]
    names:            Vec<u8>,
    #[educe(Debug(method = "str_bytes_fmt"))]
    tag:              Vec<u8>,
    end_tag:          bool,
    attribute:        [u8; 8],
    attribute_length: u8,
    encoding_seen:    bool,
    collect_encoding: bool,
    encoding:         Encoding,
    /// Whether the current `font` start tag has a `color`, `face` or `size` attribute, which makes it leave foreign content.
    font_breakout:    bool,
    /// Whether an HTML start tag that leaves foreign content, such as `<p>` in `<svg>`, has ended the region.
    breakout:         bool,
    quote:            u8,
    counter:          u8,
    declaration:      u8,
    #[educe(Debug(method = "str_bytes_fmt"))]
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
        self.font_breakout = false;
        self.breakout = false;
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

    /// Get the name of the HTML start tag that has ended the region, such as `p` for `<svg><p>`.
    pub(crate) fn breakout_tag(&self) -> Option<&[u8]> {
        self.breakout.then_some(self.tag.as_slice())
    }

    pub(crate) fn can_finish(&self) -> bool {
        // Open ordinary elements and trailing literal tag prefixes do not require closing tags.
        matches!(self.step, Step::Data | Step::TagOpen | Step::EndTagOpen)
    }

    /// Skip the bytes that cannot change the state, stopping at a CR or an ASCII control character for the caller.
    pub(crate) fn skip(&self, bytes: &[u8], mut p: usize) -> usize {
        let stops = match self.step {
            Step::Data => [b'<'; 2],
            Step::QuotedValue if !self.collect_encoding => [self.quote; 2],
            Step::BogusComment => [b'>'; 2],
            Step::Comment if self.counter == 0 => [b'-'; 2],
            Step::Cdata if self.counter == 0 => [b']'; 2],
            Step::Raw if self.counter == 0 => {
                if self.raw_end != b"</script" {
                    [b'<'; 2]
                } else if !self.script_data.can_skip() {
                    return p;
                } else if self.script_data.tracks_dash() {
                    *b"<-"
                } else {
                    [b'<'; 2]
                }
            },
            _ => return p,
        };

        while let Some(&e) = bytes.get(p) {
            if stops.contains(&e) || e == b'\r' || is_ascii_control(e) {
                break;
            }

            p += 1;
        }

        p
    }

    /// Feed the byte at `p` and the bytes after it until a CR, an ASCII control character or the end of the region.
    /// Return the position of the last consumed byte.
    pub(crate) fn feed_run(&mut self, bytes: &[u8], mut p: usize) -> usize {
        self.feed(bytes[p]);

        // The caller removes a LF after a CR, so the run stops after a CR.
        if bytes[p] == b'\r' {
            return p;
        }

        while self.active && !self.plaintext {
            let next = self.skip(bytes, p + 1);

            match bytes.get(next) {
                Some(&e) if e != b'\r' && !is_ascii_control(e) => {
                    self.feed(e);
                    p = next;
                },
                _ => return next - 1,
            }
        }

        p
    }

    fn begin_tag(&mut self, e: u8, end_tag: bool) {
        self.tag.clear();
        self.tag.push(e.to_ascii_lowercase());
        self.end_tag = end_tag;
        self.encoding_seen = false;
        self.collect_encoding = false;
        self.encoding = Encoding::default();
        self.font_breakout = false;
        self.step = Step::TagName;
    }

    fn begin_attribute(&mut self, e: u8) {
        self.attribute[0] = e.to_ascii_lowercase();
        self.attribute_length = 1;
        self.collect_encoding = false;
        self.step = Step::AttributeName;
    }

    fn end_attribute_name(&mut self) {
        // A name longer than the buffer matches no name that is checked.
        let name = self.attribute.get(..usize::from(self.attribute_length)).unwrap_or_default();

        self.collect_encoding = !self.end_tag
            && self.tag == b"annotation-xml"
            && !self.encoding_seen
            && name == b"encoding";
        self.encoding_seen |= self.collect_encoding;
        self.font_breakout |=
            !self.end_tag && self.tag == b"font" && matches!(name, b"color" | b"face" | b"size");
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

        if parent.takes_html_tag(&self.tag) {
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

    /// Close the foreign elements until an HTML element or an integration point, like browsers do for `<p>` in `<svg>`.
    /// Return `true` when the root is closed, which ends the region.
    fn break_out(&mut self) -> bool {
        while let Some(element) = self.elements.last() {
            if element.namespace == Namespace::Html || element.integration != Integration::None {
                return false;
            }

            self.names.truncate(element.start);
            self.elements.pop();
        }

        self.active = false;

        true
    }

    fn finish_tag(&mut self, self_closing: bool) {
        self.end_value();
        self.step = Step::Data;

        if self.end_tag {
            // `</br>` and `</p>` leave foreign content like the HTML start tags below.
            if matches!(self.tag.as_slice(), b"br" | b"p")
                && self.elements.last().is_some_and(|element| element.namespace != Namespace::Html)
                && self.break_out()
            {
                return;
            }

            if let Some(index) = self.elements.iter().rposition(|element| {
                self.names[element.start..element.start + element.length] == self.tag
            }) {
                self.names.truncate(self.elements[index].start);
                self.elements.truncate(index);
                self.active = !self.elements.is_empty();
            }
            return;
        }

        if (self.font_breakout || is_breakout_tag(&self.tag))
            && self.elements.last().is_some_and(|parent| !parent.takes_html_tag(&self.tag))
            && self.break_out()
        {
            self.breakout = true;
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

        if self.elements.len() == MAX_DEPTH {
            // Like the content of `plaintext`, the rest of the input is kept as it is.
            self.plaintext = true;
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
                    _ if self.tag.len() < MAX_TAG_LENGTH => self.tag.push(e.to_ascii_lowercase()),
                    _ => (),
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
                        self.counter = COMMENT_START;
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
