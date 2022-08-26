use amf;
use byteorder::ReadBytesExt;
use std::{
    io::{self, Read},
    vec,
};

pub(crate) fn be_bytes_to_u32(bytes: &[u8]) -> u32 {
    let mut result = 0_u32;
    for i in 0..bytes.len() {
        result += (bytes[i] as u32) << ((bytes.len() - i - 1) * 8);
    }
    return result;
}

fn decode_script_data(data: &[u8]) -> io::Result<Vec<amf::Amf0Value>> {
    let mut metas: Vec<amf::Amf0Value> = Vec::new();
    let cur = &mut &data[..];
    let mut decoder = amf::amf0::Decoder::new(cur);
    loop {
        match decoder.decode() {
            Ok(val) => {
                metas.push(val);
            }
            Err(e) => match e {
                amf::error::DecodeError::Io(_) => {
                    break;
                }
                _ => {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string()));
                }
            },
        }
    }
    return Ok(metas);
}

type ScriptTagData = Vec<amf::Amf0Value>;

pub trait ScriptTagDataTrait {
    fn set_duration(&mut self, duration: f64);
    fn duration(&self) -> f64;
    fn width(&self) -> f64;
    fn height(&self) -> f64;
    fn framerate(&self) -> f64;
    fn video_codec_id(&self) -> f64;
    fn audio_codec_id(&self) -> f64;
    fn read_f64(&self, key: &str) -> f64;
}

impl ScriptTagDataTrait for ScriptTagData {
    fn set_duration(&mut self, duration: f64) {
        let mut set: bool = false;
        for val in self.iter_mut() {
            if let amf::Amf0Value::EcmaArray { entries } = val {
                for kv in entries.iter_mut() {
                    if &kv.key == "duration" {
                        kv.value = amf::Amf0Value::Number(duration);
                        set = true;
                        break;
                    }
                }
                if !set {
                    entries.push(amf::Pair {
                        key: "duration".to_string(),
                        value: amf::Amf0Value::Number(duration),
                    });
                    set = true;
                }
            }
        }
        if !set {
            let mut entries = vec![];
            entries.push(amf::Pair {
                key: "duration".to_string(),
                value: amf::Amf0Value::Number(duration),
            });
            self.push(amf::Amf0Value::EcmaArray { entries });
        }
    }

    fn read_f64(&self, key: &str) -> f64 {
        for val in self.iter() {
            if let amf::Amf0Value::EcmaArray { entries } = val {
                for kv in entries.iter() {
                    if &kv.key == key {
                        if let amf::Amf0Value::Number(result) = kv.value {
                            return result;
                        }
                    }
                }
            }
        }
        return f64::INFINITY;
    }

    fn duration(&self) -> f64 {
        self.read_f64("duration")
    }

    fn width(&self) -> f64 {
        self.read_f64("width")
    }

    fn height(&self) -> f64 {
        self.read_f64("height")
    }

    fn framerate(&self) -> f64 {
        self.read_f64("framerate")
    }

    fn video_codec_id(&self) -> f64 {
        self.read_f64("videocodecid")
    }

    fn audio_codec_id(&self) -> f64 {
        self.read_f64("audiocodecid")
    }
}

#[derive(Debug, Clone)]
pub enum TagData {
    Script(ScriptTagData),
    Audio(Vec<u8>),
    Video(Vec<u8>),
}

#[derive(Debug, Clone)]
pub struct Tag {
    tp: u8,
    data_size: [u8; 3],
    timestamp: [u8; 3],
    timestamp_ex: u8,
    stream_id: [u8; 3],
    data: TagData,
}

impl Tag {
    const TYPE_AUDIO: u8 = 0x08;
    const TYPE_VIDEO: u8 = 0x09;
    const TYPE_SCRIPT: u8 = 0x12;
    pub const TAG_HEADER_SIZE: u32 = 11;

    pub fn from_reader(reader: &mut impl Read) -> io::Result<Self> {
        let tp: u8 = reader.read_u8()?;
        let mut data_size: [u8; 3] = [0; 3];
        let mut timestamp: [u8; 3] = [0; 3];
        let mut stream_id: [u8; 3] = [0; 3];
        let mut data: Vec<u8> = Vec::new();

        reader.read_exact(&mut data_size)?;
        reader.read_exact(&mut timestamp)?;
        let timestamp_ex: u8 = reader.read_u8()?;
        reader.read_exact(&mut stream_id)?;

        data.resize(be_bytes_to_u32(&data_size) as usize, 0x00);
        reader.read_exact(&mut data)?;

        if tp != Self::TYPE_AUDIO && tp != Self::TYPE_VIDEO && tp != Self::TYPE_SCRIPT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unexcepted tag type:{}", &tp),
            ));
        }

        let tag_data = if tp == Self::TYPE_AUDIO {
            TagData::Audio(data)
        } else if tp == Self::TYPE_VIDEO {
            TagData::Video(data)
        } else {
            TagData::Script(decode_script_data(&data)?)
        };

        return Ok(Self {
            tp,
            data_size,
            timestamp,
            timestamp_ex,
            stream_id,
            data: tag_data,
        });
    }

    pub fn into_bytes(&self) -> Vec<u8> {
        let mut result = vec![];
        result.push(self.tp);
        result.extend_from_slice(&self.data_size);
        result.extend_from_slice(&self.timestamp);
        result.push(self.timestamp_ex);
        result.extend_from_slice(&self.stream_id);
        match &self.data {
            TagData::Audio(data) | TagData::Video(data) => {
                result.extend_from_slice(&data);
            }
            TagData::Script(metas) => {
                for meta in metas {
                    meta.write_to(&mut result).unwrap();
                }
            }
        }
        return result;
    }

    pub fn is_video_tag(&self) -> bool {
        self.tp == Self::TYPE_VIDEO
    }

    pub fn is_audio_tag(&self) -> bool {
        self.tp == Self::TYPE_AUDIO
    }

    pub fn is_script_tag(&self) -> bool {
        self.tp == Self::TYPE_SCRIPT
    }

    pub fn tag_size(&self) -> u32 {
        be_bytes_to_u32(&self.data_size) + Self::TAG_HEADER_SIZE
    }

    pub fn timestamp(&self) -> u32 {
        be_bytes_to_u32(&self.timestamp) + ((self.timestamp_ex as u32) << 24)
    }

    pub fn set_timestamp(&mut self, timestamp: u32) {
        let bytes = timestamp.to_be_bytes();
        self.timestamp_ex = bytes[0];
        self.timestamp[0] = bytes[1];
        self.timestamp[1] = bytes[2];
        self.timestamp[2] = bytes[3];
    }

    pub fn data(&self) -> &TagData {
        &self.data
    }
}
