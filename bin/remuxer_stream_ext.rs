use bytes::BufMut;
use bytes::BytesMut;
use flv_remuxer::Message;
use futures::stream::Stream;
use futures::task::Context;
use futures::task::Poll;
use std::io;
use std::pin::Pin;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct FlvRemuxerMessageReceiver {
    inner: UnboundedReceiver<Message>,
}

impl FlvRemuxerMessageReceiver {
    pub fn new(rx: UnboundedReceiver<Message>) -> Self {
        Self { inner: rx }
    }
}

impl Stream for FlvRemuxerMessageReceiver {
    type Item = io::Result<BytesMut>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.poll_recv(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(message) => {
                if let Some(message) = message {
                    match message {
                        Message::Data(data) => {
                            let mut buf = BytesMut::with_capacity(data.len());
                            buf.put(&data[..]);
                            Poll::Ready(Some(Ok(buf)))
                        }
                        Message::Eof => Poll::Ready(None),
                        Message::Error(e) => Poll::Ready(Some(Err(e))),
                    }
                } else {
                    Poll::Ready(None)
                }
            }
        }
    }
}
