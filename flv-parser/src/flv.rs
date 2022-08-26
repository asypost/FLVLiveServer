mod header;
mod segment;
mod tag;

use byteorder::{BigEndian, ReadBytesExt};
use std::io::{self, Read};

pub use header::Header;
pub use segment::Segment;
pub use tag::{ScriptTagDataTrait, Tag, TagData};

use self::tag::be_bytes_to_u32;

pub struct Container {
    header: Header,
    body: Vec<Segment>,
}

impl Container {
    pub fn from_reader(reader: &mut impl Read) -> io::Result<Self> {
        let header = Header::from_reader(reader)?;
        let mut body = vec![];
        loop {
            if let Ok(seg) = Segment::from_reader(reader) {
                body.push(seg);
            } else {
                break;
            }
        }
        return Ok(Self { header, body });
    }
}

///Parser State
enum ParserState {
    ///Parsing Header with byte count required
    Header(usize),
    ///Parsing PreTagSize with byte count required
    PreTagSize(usize),
    ///Parsing Tag with byte count required
    Tag(usize),
}

///Parse result of the parser
pub enum ParseResult {
    ///The parser need more data
    MoreDataRequired(usize),
    ///A flv header found
    Header(Header),
    ///A previous tag size found
    PreTagSize(u32),
    ///A tag found
    Tag(Tag),
}

///flv parser
pub struct Parser {
    state: ParserState,
    buffer: Vec<u8>,
}

impl Parser {
    ///Create a new praser
    pub fn new() -> Self {
        Self {
            state: ParserState::Header(Header::HEADER_SIZE as usize),
            buffer: vec![],
        }
    }

    ///Feed the parser with some data
    pub fn feed(&mut self, data: &[u8]) {
        if data.len() > 0 {
            self.buffer.extend_from_slice(data);
        }
    }

    ///Start parse the data in buffer,you need to call this util it
    ///returns a ParserResult::MoreDataRequired.
    /// #Example
    ///```
    ///loop {
    ///  parser.feed(&buf);
    ///  loop {
    ///    match parser.parse() {
    ///      ParseResult::MoreDataRequired(size) => {
    ///          break;
    ///       },
    ///       ParserState::Header(header) => {
    ///
    ///       },
    ///       ParserState::PreTagSize(size) => {
    ///
    ///       },
    ///       ParserState::Tag(tag) => {
    ///
    ///       }
    ///    }    
    ///  }
    ///}
    ///```
    pub fn parse(&mut self) -> io::Result<ParseResult> {
        match self.state {
            ParserState::Header(required) => {
                if required > self.buffer.len() {
                    return Ok(ParseResult::MoreDataRequired(required - self.buffer.len()));
                }
                let header = Header::from_reader(&mut &self.buffer[..required])?;
                let _ = self.buffer.drain(..required);
                self.state = ParserState::PreTagSize(std::mem::size_of::<u32>());
                return Ok(ParseResult::Header(header));
            }
            ParserState::PreTagSize(required) => {
                if required > self.buffer.len() {
                    return Ok(ParseResult::MoreDataRequired(required - self.buffer.len()));
                }
                let pre_tag_size = (&mut &self.buffer[..required]).read_u32::<BigEndian>()?;
                let _ = self.buffer.drain(..required);
                self.state = ParserState::Tag(Tag::TAG_HEADER_SIZE as usize);
                return Ok(ParseResult::PreTagSize(pre_tag_size));
            }
            ParserState::Tag(required) => {
                if required > self.buffer.len() {
                    return Ok(ParseResult::MoreDataRequired(required - self.buffer.len()));
                }
                if required == Tag::TAG_HEADER_SIZE as usize {
                    let data_size = be_bytes_to_u32(&self.buffer[1..4]);
                    if data_size == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "Zero sized tag data",
                        ));
                    }
                    let tag_size = required + data_size as usize;
                    if tag_size > self.buffer.len() {
                        self.state = ParserState::Tag(tag_size);
                        return Ok(ParseResult::MoreDataRequired(tag_size - self.buffer.len()));
                    } else {
                        return self.parse_tag(tag_size);
                    }
                } else {
                    return self.parse_tag(required);
                }
            }
        }
    }

    #[inline(always)]
    fn parse_tag(&mut self, required: usize) -> io::Result<ParseResult> {
        let tag = Tag::from_reader(&mut &self.buffer[..required])?;
        let _ = self.buffer.drain(..required);
        self.state = ParserState::PreTagSize(std::mem::size_of::<u32>());
        return Ok(ParseResult::Tag(tag));
    }
}
