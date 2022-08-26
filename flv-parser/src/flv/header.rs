use byteorder::{BigEndian, ReadBytesExt};
use std::io::{self, Read};

///flv header
#[derive(Debug, Clone)]
pub struct Header {
    signature: [u8; 3],
    version: u8,
    flags: u8,
    header_size: u32,
}

impl Header {
    pub const HEADER_SIGNATURE: [u8; 3] = [0x46, 0x4C, 0x56];
    pub const HEADER_SIZE: u32 = 0x09;
    const HEADER_VIDEO_FLAG: u8 = 0b00000001;
    const HEADER_AUDIO_FLAG: u8 = 0b00000100;

    ///Build a Header from something implements Read trait
    pub fn from_reader(reader: &mut impl Read) -> io::Result<Self> {
        let mut header = Header {
            signature: [0; 3],
            version: 0x01,
            flags: 0x00,
            header_size: Self::HEADER_SIZE,
        };
        reader.read_exact(&mut header.signature)?;
        header.version = reader.read_u8()?;
        header.flags = reader.read_u8()?;
        header.header_size = reader.read_u32::<BigEndian>()?;

        if header.signature != Self::HEADER_SIGNATURE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unexcepted signature:{:?}", &header.signature),
            ));
        }
        if header.size() != Self::HEADER_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unexcepted size:{}", &header.size()),
            ));
        }

        Ok(header)
    }

    ///The version of flv
    pub fn version(&self) -> u8 {
        self.version
    }

    ///The size of header
    pub fn size(&self) -> u32 {
        self.header_size
    }

    ///Indicates that the flv header flags has video bit set
    pub fn has_video(&self) -> bool {
        self.flags & Self::HEADER_VIDEO_FLAG == Self::HEADER_VIDEO_FLAG
    }

    ///Indicates that the flv header flags has audio bit set
    pub fn has_audio(&self) -> bool {
        self.flags & Self::HEADER_AUDIO_FLAG == Self::HEADER_AUDIO_FLAG
    }

    ///Set video bit of flags
    pub fn set_has_video(&mut self, has_video: bool) {
        let flag = if has_video { 0b11111111 } else { 0b11111110 };
        self.flags &= flag;
    }

    ///Set audio bit of flags
    pub fn set_has_audio(&mut self, has_audio: bool) {
        let flag = if has_audio { 0b11111111 } else { 0b11111011 };
        self.flags &= flag;
    }

    ///Return the signature of flv.It should be "FLV"
    pub fn signature(&self) -> String {
        let mut s = String::with_capacity(3);
        s.insert(0, *self.signature.get(0).unwrap() as char);
        s.insert(1, *self.signature.get(1).unwrap() as char);
        s.insert(2, *self.signature.get(2).unwrap() as char);
        return s;
    }

    ///Return the bytes of this header
    pub fn into_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::<u8>::with_capacity(9);
        bytes.extend_from_slice(&self.signature);
        bytes.push(self.version);
        bytes.push(self.flags);
        bytes.extend_from_slice(&self.header_size.to_be_bytes());
        return bytes;
    }
}
