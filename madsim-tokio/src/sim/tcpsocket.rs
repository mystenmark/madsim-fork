#![allow(unused_variables)]

use std::{
    cell::Cell,
    io,
    net::SocketAddr,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};

use crate::net::{TcpStream, TcpListener};

pub struct TcpSocket {
    bind_addr: Cell<Option<SocketAddr>>
}

impl TcpSocket {
    pub fn new_v4() -> io::Result<TcpSocket> {
        TcpSocket::new()
    }

    pub fn new_v6() -> io::Result<TcpSocket> {
        TcpSocket::new()
    }

    fn new() -> io::Result<TcpSocket> {
        Ok(TcpSocket { bind_addr: Cell::new(None) })
    }

    pub fn set_reuseaddr(&self, reuseaddr: bool) -> io::Result<()> {
        todo!()
    }

    pub fn reuseaddr(&self) -> io::Result<bool> {
        todo!()
    }

    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    #[cfg_attr(
        docsrs,
        doc(cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos"))))
    )]
    pub fn set_reuseport(&self, reuseport: bool) -> io::Result<()> {
        todo!()
    }

    #[cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos")))]
    #[cfg_attr(
        docsrs,
        doc(cfg(all(unix, not(target_os = "solaris"), not(target_os = "illumos"))))
    )]
    pub fn reuseport(&self) -> io::Result<bool> {
        todo!()
    }

    pub fn set_send_buffer_size(&self, size: u32) -> io::Result<()> {
        todo!()
    }

    pub fn send_buffer_size(&self) -> io::Result<u32> {
        todo!()
    }

    pub fn set_recv_buffer_size(&self, size: u32) -> io::Result<()> {
        todo!()
    }

    pub fn recv_buffer_size(&self) -> io::Result<u32> {
        todo!()
    }

    pub fn set_linger(&self, dur: Option<Duration>) -> io::Result<()> {
        todo!()
    }

    pub fn linger(&self) -> io::Result<Option<Duration>> {
        todo!()
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        Ok(self.bind_addr.get().unwrap())
    }

    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        todo!()
    }

    pub fn bind(&self, addr: SocketAddr) -> io::Result<()> {
        assert!(self.bind_addr.get().is_none());
        self.bind_addr.set(Some(addr));
        Ok(())
    }

    pub async fn connect(self, addr: SocketAddr) -> io::Result<TcpStream> {
        TcpStream::connect(addr).await
    }

    pub fn listen(self, backlog: u32) -> io::Result<TcpListener> {
        todo!()
        // we need a way to bind synchronously, which just means removing
        // the random delay from Endpoint::bind()
        //TcpListener::bind(self.bind_addr.get().unwrap())
    }

    pub fn from_std_stream(std_stream: std::net::TcpStream) -> TcpSocket {
        unimplemented!("can't make simulated tcp socket from std stream")
    }
}

#[cfg(unix)]
impl AsRawFd for TcpSocket {
    fn as_raw_fd(&self) -> RawFd {
        unimplemented!("can't view simulated TcpSocket as raw fd")
    }
}

#[cfg(unix)]
impl FromRawFd for TcpSocket {
    /// Converts a `RawFd` to a `TcpSocket`.
    ///
    /// # Notes
    ///
    /// The caller is responsible for ensuring that the socket is in
    /// non-blocking mode.
    unsafe fn from_raw_fd(fd: RawFd) -> TcpSocket {
        unimplemented!("can't make simulated TcpSocket from raw fd")
    }
}

#[cfg(unix)]
impl IntoRawFd for TcpSocket {
    fn into_raw_fd(self) -> RawFd {
        unimplemented!("can't make simulated TcpSocket into raw fd")
    }
}

