use std::{io::{self, IoSlice}, pin::Pin, task::{Context, Poll}};

use pin_project::pin_project;
use tokio::{io::{AsyncRead, AsyncWrite, ReadBuf}, net::TcpStream};
use tokio_rustls::server::TlsStream;

#[pin_project(project = ClientStreamProj)]
/// A wrapper type around either:
pub enum ClientStream {
    /// A normal TCP stream
    Plain(TcpStream),
    /// A TLS Server stream
    Rustls(Box<TlsStream<TcpStream>>),
}

macro_rules! project_fn {
    ($key:ident($($arg:ident: $ty:ty),*) -> $res:ty) => {
        fn $key(self: Pin<&mut Self>, $($arg: $ty),*) -> Poll<$res> {
            match self.project() {
                ClientStreamProj::Plain(pointer) => Pin::new(pointer).$key($($arg),*),
                ClientStreamProj::Rustls(pointer) => Pin::new(pointer).$key($($arg),*),
            }
        }    
    };
}

impl AsyncRead for ClientStream {
    project_fn!(poll_read(cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> io::Result<()>);
}

impl AsyncWrite for ClientStream {
    project_fn!(poll_write(cx: &mut Context<'_>, buf: &[u8]) -> Result<usize, io::Error>);
    project_fn!(poll_flush(cx: &mut Context<'_>) -> Result<(), io::Error>);
    project_fn!(poll_shutdown(cx: &mut Context<'_>) -> Result<(), io::Error>);
    project_fn!(poll_write_vectored(cx: &mut Context<'_>, bufs: &[IoSlice<'_>]) -> Result<usize, io::Error>);
}