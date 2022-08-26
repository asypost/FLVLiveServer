use super::subscriber::Message;
use super::subscriber::Subscriber;
use super::transcoder::Transcoder;
use super::transcoder::TranscoderOptions;
use flv_parser::flv::Header;
use flv_parser::flv::ParseResult;
use flv_parser::flv::Parser;
use flv_parser::flv::Tag;
use flv_parser::flv::TagData;
use std::io;
use std::time::Duration;
use std::vec;
use tokio::sync::mpsc::{self, UnboundedReceiver};

#[derive(Debug, Clone, Copy)]
pub enum RemuxerState {
    Idle,
    Pending,
    Dead,
}

//                                                                 +----------+
//                                                           +---->|subscriber|
//                                                           |     +----------+
//                                                           |
// +--------------+     +-------------------+   +----------+ |     +----------+
// | video source +---->|ffmpeg transcoding +-->| remuxer  +-+---->|subscriber|
// +--------------+     +-------------------+   +----------+ |     +----------+
//                                                           |
//                                                           |     +----------+
//                                                           +---->|subscriber|
//                                                                 +----------+
pub struct Remuxer {
    transcoder: Transcoder,
    subscribers: Vec<Subscriber>,
    flv_parser: Parser,
    flv_header: Option<Header>,
    flv_script_tag: Option<Tag>,
    avc_decoder_configuration_record: Option<Tag>,
    audio_specific_config: Option<Tag>,
    buffer: Vec<u8>,
    rw_timeout: Option<u64>,
    state: RemuxerState,
}

impl Remuxer {
    pub fn new_with_timeout_and_options(
        source: &str,
        rw_timeout_micros: Option<u64>,
        transcoder_options: Option<TranscoderOptions>,
    ) -> Self {
        let mut buffer: Vec<u8> = Vec::new();
        buffer.resize_with(200 * 1024, Default::default);
        Self {
            transcoder: Transcoder::with_option(source, transcoder_options),
            subscribers: vec![],
            flv_parser: Parser::new(),
            flv_header: None,
            flv_script_tag: None,
            audio_specific_config: None,
            avc_decoder_configuration_record: None,
            buffer,
            rw_timeout: rw_timeout_micros,
            state: RemuxerState::Idle,
        }
    }

    pub fn subscribe(&mut self, address: &str) -> UnboundedReceiver<Message> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.push(Subscriber::new(tx, address));
        rx
    }

    fn dispatch_flv_header_script(&mut self) {
        for subscriber in &mut self.subscribers {
            if subscriber.header_script_sended {
                continue;
            }
            //send flv header
            let header_data = self.flv_header.as_ref().unwrap().into_bytes();
            if let Err(_) = subscriber.send(Message::Data(header_data)) {
                subscriber.active = false;
            } else {
                //send flv pre tag size,here is 0
                if let Err(_) = subscriber.send(Message::Data(0_u32.to_be_bytes().to_vec())) {
                    subscriber.active = false;
                } else {
                    //send flv script tag
                    let data = self.flv_script_tag.as_ref().unwrap().into_bytes();
                    let pre_tag_size = data.len() as u32;
                    subscriber.pre_tag_size = pre_tag_size;
                    if let Err(_) = subscriber.send(Message::Data(data)) {
                        subscriber.active = false;
                    }
                    subscriber.header_script_sended = true;
                }
            }
        }
        self.clean_subscriber();
    }

    #[inline]
    fn is_avc_decoder_configuration_record(&self, tag: &Tag) -> bool {
        if tag.is_video_tag() {
            if let TagData::Video(tag_data) = tag.data() {
                if tag_data.len() < 2 {
                    return false;
                }
                let codec = tag_data.get(0).unwrap() & 0x0f;
                if codec != 0x07 {
                    return false;
                }
                let packet_type = (tag_data.get(1)).unwrap();
                return *packet_type == 0x00;
            }
        }
        return false;
    }

    #[inline]
    fn is_video_tag_keyframe(tag: &Tag) -> bool {
        if let TagData::Video(tag_data) = tag.data() {
            if tag_data.len() < 2 {
                return false;
            }
            let frame_type = (tag_data.get(0).unwrap() & 0xf0) >> 4;
            return frame_type == 0x01;
        }
        return false;
    }

    #[inline]
    fn is_audio_specific_config(&self, tag: &Tag) -> bool {
        if tag.is_audio_tag() {
            if let TagData::Audio(tag_data) = tag.data() {
                if tag_data.len() < 2 {
                    return false;
                }
                let sound_format = (tag_data.get(0).unwrap() & 0xf0) >> 4;
                if sound_format != 0x0a {
                    return false;
                }
                let packet_type = (tag_data.get(1)).unwrap();
                return *packet_type == 0x00;
            }
        }
        return false;
    }

    fn clean_subscriber(&mut self) {
        for subscriber in &self.subscribers {
            if subscriber.active == false {
                let message = format!("{} unsubscribed", subscriber.id);
                log::info!("{}", &message);
            }
        }
        self.subscribers.retain(|item| item.active);
    }

    fn dispatch_audio_video_codec_tag(&mut self) {
        for subscriber in &mut self.subscribers {
            if subscriber.video_codec_tag_sended == false
                && self.avc_decoder_configuration_record.is_some()
            {
                let tag = self.avc_decoder_configuration_record.as_mut().unwrap();
                let tag_timestamp = tag.timestamp();
                let pre_tag_size_data: Vec<u8> = subscriber.pre_tag_size.to_be_bytes().into();

                if let Err(_) = subscriber.send(Message::Data(pre_tag_size_data)) {
                    subscriber.active = false;
                } else {
                    if subscriber.video_start_timestamp.is_none() {
                        tag.set_timestamp(0);
                    } else {
                        tag.set_timestamp(
                            tag_timestamp - subscriber.video_start_timestamp.unwrap(),
                        );
                    }
                    let tag_data = tag.into_bytes();
                    subscriber.pre_tag_size = tag_data.len() as u32;
                    tag.set_timestamp(tag_timestamp);
                    if let Err(_) = subscriber.send(Message::Data(tag_data)) {
                        subscriber.active = false;
                    }
                    subscriber.video_codec_tag_sended = true;
                }
            }
            if subscriber.audio_codec_tag_sended == false && self.audio_specific_config.is_some() {
                let tag = self.audio_specific_config.as_mut().unwrap();
                let tag_timestamp = tag.timestamp();
                let pre_tag_size_data: Vec<u8> = subscriber.pre_tag_size.to_be_bytes().into();

                if let Err(_) = subscriber.send(Message::Data(pre_tag_size_data)) {
                    subscriber.active = false;
                } else {
                    if subscriber.audio_start_timestamp.is_none() {
                        tag.set_timestamp(0);
                    } else {
                        tag.set_timestamp(
                            tag_timestamp - subscriber.audio_start_timestamp.unwrap(),
                        );
                    }
                    let tag_data = tag.into_bytes();
                    subscriber.pre_tag_size = tag_data.len() as u32;
                    tag.set_timestamp(tag_timestamp);
                    if let Err(_) = subscriber.send(Message::Data(tag_data)) {
                        subscriber.active = false;
                    }
                    subscriber.audio_codec_tag_sended = true;
                }
            }
        }
        self.clean_subscriber();
    }

    fn dispatch_tag(&mut self, tag: &mut Tag) {
        let tag_timestamp: u32 = tag.timestamp();

        for subscriber in &mut self.subscribers {
            //send pre tag size
            let pre_tag_size_data = subscriber.pre_tag_size.to_be_bytes().into();
            //send tag data and set pre tag size
            if tag.is_audio_tag() {
                let has_video = if let Some(header) = self.flv_header.as_ref() {
                    header.has_video()
                } else {
                    false
                };
                let has_video_codec = self.avc_decoder_configuration_record.is_some();
                let video_trasmission_started = subscriber.video_start_timestamp.is_some();
                if video_trasmission_started || !has_video_codec || !has_video {
                    if subscriber.audio_start_timestamp.is_none() {
                        subscriber.audio_start_timestamp = Some(tag_timestamp);
                        tag.set_timestamp(0);
                    } else {
                        tag.set_timestamp(
                            tag_timestamp - subscriber.audio_start_timestamp.unwrap(),
                        );
                    }

                    if let Err(_) = subscriber.send(Message::Data(pre_tag_size_data)) {
                        subscriber.active = false;
                    } else {
                        let tag_data = tag.into_bytes();
                        subscriber.pre_tag_size = tag_data.len() as u32;

                        tag.set_timestamp(tag_timestamp);
                        if let Err(_) = subscriber.send(Message::Data(tag_data)) {
                            subscriber.active = false;
                        }
                    }
                }
            } else if tag.is_video_tag() {
                if subscriber.video_start_timestamp.is_none() {
                    //maybe send a key frame for the first time and solve the sync problem(not work)
                    if Self::is_video_tag_keyframe(tag) {
                        subscriber.video_start_timestamp = Some(tag_timestamp);
                        if let Err(_) = subscriber.send(Message::Data(pre_tag_size_data)) {
                            subscriber.active = false;
                        } else {
                            tag.set_timestamp(0);
                            let tag_data = tag.into_bytes();
                            subscriber.pre_tag_size = tag_data.len() as u32;
                            tag.set_timestamp(tag_timestamp);
                            if let Err(_) = subscriber.send(Message::Data(tag_data)) {
                                subscriber.active = false;
                            }
                        }
                    }
                } else {
                    if let Err(_) = subscriber.send(Message::Data(pre_tag_size_data)) {
                        subscriber.active = false;
                    } else {
                        tag.set_timestamp(
                            tag_timestamp - subscriber.video_start_timestamp.unwrap(),
                        );
                        let tag_data = tag.into_bytes();
                        subscriber.pre_tag_size = tag_data.len() as u32;
                        tag.set_timestamp(tag_timestamp);
                        if let Err(_) = subscriber.send(Message::Data(tag_data)) {
                            subscriber.active = false;
                        }
                    }
                }
            }
        }
        self.clean_subscriber();
    }

    async fn dispatch_eof(&mut self) {
        for subscriber in &self.subscribers {
            let _ = subscriber.send(Message::Eof);
        }
        self.subscribers.clear();
    }

    async fn dispatch_error(&mut self, error: &io::Error) {
        for subscriber in &self.subscribers {
            let _ = subscriber.send(Message::Error(io::Error::new(
                error.kind(),
                error.to_string(),
            )));
        }
        self.subscribers.clear();
    }

    fn handle_flv_parser_result(&mut self, parse_result: ParseResult) -> bool {
        match parse_result {
            ParseResult::MoreDataRequired(_) => {
                return true;
            }
            ParseResult::Header(header) => {
                self.flv_header = Some(header);
            }
            ParseResult::Tag(mut tag) => {
                if tag.is_script_tag() && self.flv_script_tag.is_none() {
                    self.flv_script_tag = Some(tag);
                } else {
                    if self.flv_script_tag.is_some() && self.flv_header.is_some() {
                        self.dispatch_flv_header_script();
                    }
                    if self.is_avc_decoder_configuration_record(&tag)
                        && self.avc_decoder_configuration_record.is_none()
                    {
                        self.avc_decoder_configuration_record = Some(tag);
                    } else if self.is_audio_specific_config(&tag)
                        && self.audio_specific_config.is_none()
                    {
                        self.audio_specific_config = Some(tag);
                    } else {
                        self.dispatch_audio_video_codec_tag();
                        self.dispatch_tag(&mut tag);
                    }
                }
            }
            ParseResult::PreTagSize(_) => {
                //do nothing
            }
        }
        return false;
    }

    pub fn state(&self) -> RemuxerState {
        self.state
    }

    pub fn set_state(&mut self, state: RemuxerState) {
        self.state = state;
    }

    pub async fn next(&mut self) -> io::Result<()> {
        if self.subscribers.len() == 0 {
            self.state = RemuxerState::Dead;
            return Err(io::Error::new(io::ErrorKind::NotConnected, "no subscriber"));
        }
        let result = if let Some(timeout) = self.rw_timeout {
            tokio::time::timeout(
                Duration::from_micros(timeout),
                self.transcoder.pull_data(&mut self.buffer),
            )
            .await?
        } else {
            self.transcoder.pull_data(&mut self.buffer).await
        };
        match result {
            Ok(size) => {
                if size > 0 {
                    self.flv_parser.feed(&self.buffer[..size]);
                    loop {
                        let parse_result = self.flv_parser.parse()?;
                        if self.handle_flv_parser_result(parse_result) {
                            break;
                        }
                    }
                } else {
                    self.state = RemuxerState::Dead;
                    self.transcoder.stop().await?;
                    self.dispatch_eof().await;
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "End of stream"));
                }
            }
            Err(e) => {
                self.state = RemuxerState::Dead;
                self.transcoder.stop().await?;
                self.dispatch_error(&e).await;
                return Err(e);
            }
        }

        Ok(())
    }
}
