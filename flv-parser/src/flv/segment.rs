use super::tag::{Tag, TagData};
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{self, Read};

#[derive(Debug, Clone)]
pub struct Segment {
    pre_tag_size: u32,
    tag: Option<Tag>,
}

impl Segment {
    pub fn from_reader(reader: &mut impl Read) -> io::Result<Self> {
        let mut tag = None;
        let pre_tag_size = reader.read_u32::<BigEndian>()?;
        match Tag::from_reader(reader) {
            Ok(t) => {
                tag = Some(t);
            }
            Err(e) => match e.kind() {
                io::ErrorKind::UnexpectedEof => {}
                _ => {
                    return Err(e);
                }
            },
        }
        return Ok(Self { pre_tag_size, tag });
    }

    pub fn tag(&self) -> &Option<Tag> {
        &self.tag
    }

    pub fn has_tag(&self) -> bool {
        self.tag.is_some()
    }

    pub fn has_video_tag(&self) -> bool {
        if self.tag.is_some() {
            match self.tag.as_ref().unwrap().data() {
                &TagData::Video(_) => {
                    return true;
                }
                _ => {
                    return false;
                }
            }
        }
        return false;
    }

    pub fn has_script_tag(&self) -> bool {
        if self.tag.is_some() {
            match self.tag.as_ref().unwrap().data() {
                &TagData::Script(_) => {
                    return true;
                }
                _ => {
                    return false;
                }
            }
        }
        return false;
    }

    pub fn has_audio_tag(&self) -> bool {
        if self.tag.is_some() {
            match self.tag.as_ref().unwrap().data() {
                &TagData::Audio(_) => {
                    return true;
                }
                _ => {
                    return false;
                }
            }
        }
        return false;
    }

    pub fn tag_mut(&mut self) -> &mut Option<Tag> {
        &mut self.tag
    }

    pub fn pre_tag_size(&self) -> u32 {
        self.pre_tag_size
    }

    pub fn set_pre_tag_size(&mut self, size: u32) {
        self.pre_tag_size = size;
    }

    pub fn into_bytes(&self) -> Vec<u8> {
        if self.has_tag() {
            let mut result = self.tag.as_ref().unwrap().into_bytes();
            let pre_tag_size_bytes = self.pre_tag_size.to_be_bytes();
            for i in 0..pre_tag_size_bytes.len() {
                result.insert(i, pre_tag_size_bytes[i]);
            }
            return result;
        }
        return self.pre_tag_size.to_be_bytes().into();
    }
}
