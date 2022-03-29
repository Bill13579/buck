// pointer_events.rs
// Pointer events (touch screen events) handler

use std::any::Any;
use std::borrow::BorrowMut;
use std::fs::{OpenOptions, File};
use std::io::{Cursor, Read};
use std::path::PathBuf;
use std::sync::{RwLock, Arc, Mutex};
use std::thread::{self, Thread, JoinHandle};
use std::time::Duration;
use byteorder::{NativeEndian, ReadBytesExt};
use std::sync::mpsc::{self, Sender, Receiver, SendError, SyncSender};
use evdev::{Device, FetchEventsSynced, InputEvent};

use crate::read_config::root;

use nix::{
    fcntl::{FcntlArg, OFlag},
    sys::epoll,
};
use std::os::unix::io::{AsRawFd, RawFd};

struct Epoll(RawFd);

impl Epoll {
    pub(crate) fn new(fd: RawFd) -> Self {
        Epoll(fd)
    }
}

impl AsRawFd for Epoll {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        let _ = nix::unistd::close(self.0);
    }
}

pub struct PointerEventsReader {
    dev: Arc<Mutex<Device>>,
    update_thread: Option<JoinHandle<()>>,
    buffer: Arc<Mutex<Vec<InputEvent>>>,
    tx: SyncSender<usize>,
    rx: Arc<Mutex<Receiver<usize>>>
}

impl PointerEventsReader {
    pub fn new(event_path: PathBuf) -> PointerEventsReader {
        let mut dev = Device::open(event_path).unwrap();
        let (tx, rx) = mpsc::sync_channel(0);
        let mut p = PointerEventsReader { dev: Arc::new(Mutex::new(dev)), update_thread: None, buffer: Arc::new(Mutex::new(Vec::new())), tx, rx: Arc::new(Mutex::new(rx)) };
        p
    }
    pub fn start_thread(&mut self) {
        let mut dev_clone = self.dev.clone();
        let mut buffer_clone = self.buffer.clone();
        let mut rx_clone = self.rx.clone();
        self.update_thread = Some(thread::spawn(move || {
            let mut dev_lock = dev_clone.lock().unwrap();
            let mut rx_lock = rx_clone.lock().unwrap();
            let mut events = [epoll::EpollEvent::empty(); 2];
            let raw_fd = dev_lock.as_raw_fd();
            // Set nonblocking
            nix::fcntl::fcntl(raw_fd, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("error setting nonblocking");

            // Create epoll handle and attach raw_fd
            let epoll_fd = Epoll::new(epoll::epoll_create1(
                epoll::EpollCreateFlags::EPOLL_CLOEXEC,
            ).expect("error creating and attaching raw_fd"));
            let mut event = epoll::EpollEvent::new(epoll::EpollFlags::EPOLLIN, 0);
            epoll::epoll_ctl(
                epoll_fd.as_raw_fd(),
                epoll::EpollOp::EpollCtlAdd,
                raw_fd,
                Some(&mut event),
            ).expect("error epoll_ctl'ing");
            loop {
                match dev_lock.fetch_events() {
                    Ok(iterator) => {
                        let mut buffer_lock = buffer_clone.lock().unwrap();
                        for e in iterator {
                            buffer_lock.push(e);
                        }
                    },
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        epoll::epoll_wait(epoll_fd.as_raw_fd(), &mut events, -1);
                    },
                    Err(e) => {
                        eprintln!("{}", e);
                        break;
                    }
                }
                if let Ok(_) = rx_lock.recv_timeout(Duration::from_millis(10)) {
                    break;
                }
            }
        }));
    }
    pub fn end_thread(&mut self) {
        self.tx.send(0);
    }
    pub fn next(&mut self) -> Option<InputEvent> {
        let mut buffer_lock = self.buffer.lock().unwrap();
        buffer_lock.pop()
    }
    pub fn grab(&mut self) -> Result<(), std::io::Error> {
        self.dev.lock().unwrap().grab()
    }
    pub fn ungrab(&mut self) -> Result<(), std::io::Error> {
        self.dev.lock().unwrap().ungrab()
    }
}

#[derive(Copy, Clone)]
pub struct Coords {
    pub x: u32, pub y: u32
}

impl std::fmt::Display for Coords {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}

#[derive(Copy, Clone)]
pub enum CapturedPointerEvent {
    PointerOn(Coords),
    PointerOff(Coords)
}

pub struct PointerEventsKeeper {
    reader: PointerEventsReader,
    x: Arc<RwLock<u32>>,
    y: Arc<RwLock<u32>>,
    viewport_width: u32,
    viewport_height: u32,
    tx: Sender<CapturedPointerEvent>,
    pub rx: Receiver<CapturedPointerEvent>
}

impl PointerEventsKeeper {
    pub fn new(event_filename: PathBuf, viewport_width: u32, viewport_height: u32) -> PointerEventsKeeper {
        let reader = PointerEventsReader::new(event_filename.clone());
        let x = Arc::new(RwLock::new(0));
        let y = Arc::new(RwLock::new(0));
        let (tx, rx) = mpsc::channel::<CapturedPointerEvent>();
        
        let keeper = PointerEventsKeeper { reader, x, y, viewport_width, viewport_height, tx, rx };

        keeper
    }
    pub fn check_input(&mut self) {
        let mut evtopt = self.reader.next();
        while let Some(e) = evtopt {
            let mut x_borrow = self.x.write().unwrap();
            let mut y_borrow = self.y.write().unwrap();
            match e.code() {
                53 => {
                    if cfg!(feature = "kindle") {
                        *x_borrow = ((e.value() as f64 / 4096.0) * self.viewport_width as f64).round() as u32;
                    } else {
                        *y_borrow = e.value() as u32;
                        println!("{}", y_borrow);
                    }
                },
                54 => {
                    if cfg!(feature = "kindle") {
                        *y_borrow = ((e.value() as f64 / 4096.0) * self.viewport_height as f64).round() as u32;
                    } else {
                        *x_borrow = e.value() as u32;
                        println!("{}", x_borrow);
                    }
                },
                57 => {
                    if e.value() == -1i32 {
                        self.tx.send(CapturedPointerEvent::PointerOn(Coords { x: *x_borrow, y: *y_borrow }));
                    } else if e.value() == 0x0 {
                        self.tx.send(CapturedPointerEvent::PointerOff(Coords { x: *x_borrow, y: *y_borrow }));
                    }
                },
                _ => {
                    println!("unknown event values {} {}", e.code(), e.value());
                }
            }
            evtopt = self.reader.next();
        }
    }
    pub fn coords(&self) -> Coords {
        let x_borrow = self.x.read().unwrap();
        let y_borrow = self.y.read().unwrap();
        Coords { x: *x_borrow, y: *y_borrow }
    }
    pub fn grab(&mut self) -> Result<(), std::io::Error> {
        self.reader.grab()
    }
    pub fn ungrab(&mut self) -> Result<(), std::io::Error> {
        self.reader.ungrab()
    }
    pub fn start_thread(&mut self) {
        self.reader.start_thread();
    }
    pub fn end_thread(&mut self) {
        self.reader.end_thread();
    }
}
