use std::fs::File;
use std::future::Future;
use std::io;
use std::io::{BufReader, ErrorKind};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, ready};

use hyper::server::accept::Accept;
use hyper::server::conn::{AddrIncoming, AddrStream};
use rustls::ServerConfig;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub fn load_certs(filename: &str) -> io::Result<Vec<rustls::Certificate>> {
    let certfile = File::open(filename)
        .map_err(|e| io::Error::new(ErrorKind::Other, format!("failed to open {}: {}", filename, e)))?;
    let mut reader = BufReader::new(certfile);

    let certs = rustls_pemfile::certs(&mut reader)
        .map_err(|_| io::Error::new(ErrorKind::Other, "failed to load certificate".to_string()))?;
    Ok(certs
        .into_iter()
        .map(rustls::Certificate)
        .collect())
}

pub fn load_private_key(filename: &str) -> io::Result<rustls::PrivateKey> {
    let keyfile = File::open(filename)
        .map_err(|e| io::Error::new(ErrorKind::Other, format!("failed to open {}: {}", filename, e)))?;
    let mut reader = BufReader::new(keyfile);

    let keys = rustls_pemfile::ec_private_keys(&mut reader)
        .or_else(|_| {
            let mut reader = BufReader::new(File::open(filename).unwrap());
            rustls_pemfile::rsa_private_keys(&mut reader)
        })
        .or_else(|_| {
            let mut reader = BufReader::new(File::open(filename).unwrap());
            rustls_pemfile::pkcs8_private_keys(&mut reader)
        })
        .map_err(|_| io::Error::new(ErrorKind::Other, "failed to load private key".to_string()))?;
    if keys.len() != 1 {
        return Err(io::Error::new(ErrorKind::Other, "expected a single private key".to_string()));
    }

    Ok(rustls::PrivateKey(keys[0].clone()))
}

enum State {
    Handshaking(tokio_rustls::Accept<AddrStream>),
    Streaming(tokio_rustls::server::TlsStream<AddrStream>),
}

pub struct TlsStream {
    state: State,
}

impl TlsStream {
    fn new(stream: AddrStream, config: Arc<ServerConfig>) -> TlsStream {
        let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
        TlsStream {
            state: State::Handshaking(accept),
        }
    }
}

impl AsyncRead for TlsStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf,
    ) -> Poll<io::Result<()>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_read(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for TlsStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let pin = self.get_mut();
        match pin.state {
            State::Handshaking(ref mut accept) => match ready!(Pin::new(accept).poll(cx)) {
                Ok(mut stream) => {
                    let result = Pin::new(&mut stream).poll_write(cx, buf);
                    pin.state = State::Streaming(stream);
                    result
                }
                Err(err) => Poll::Ready(Err(err)),
            },
            State::Streaming(ref mut stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.state {
            State::Handshaking(_) => Poll::Ready(Ok(())),
            State::Streaming(ref mut stream) => Pin::new(stream).poll_shutdown(cx),
        }
    }
}

pub struct TlsAcceptor {
    config: Arc<ServerConfig>,
    incoming: AddrIncoming,
}

impl TlsAcceptor {
    pub fn new(config: Arc<ServerConfig>, incoming: AddrIncoming) -> TlsAcceptor {
        TlsAcceptor { config, incoming }
    }
}

impl Accept for TlsAcceptor {
    type Conn = TlsStream;
    type Error = io::Error;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let pin = self.get_mut();
        match ready!(Pin::new(&mut pin.incoming).poll_accept(cx)) {
            Some(Ok(sock)) => Poll::Ready(Some(Ok(TlsStream::new(sock, pin.config.clone())))),
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => Poll::Ready(None),
        }
    }
}