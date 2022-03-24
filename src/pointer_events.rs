use std::borrow::BorrowMut;
use std::fs::{OpenOptions, File};
use std::io::{Cursor, Read};
use std::sync::{RwLock, Arc, Mutex};
use std::thread;
use byteorder::{NativeEndian, ReadBytesExt};
use std::sync::mpsc::{self, Sender, Receiver};

pub struct PointerEvent {
    pub ts: u64,
    pub e_type: u16,
    pub e_code: u16,
    pub e_value: u32
}

pub struct PointerEventsReader {
    dev_file: File
}

impl PointerEventsReader {
    pub fn new(event_filename: &str) -> PointerEventsReader {
        let mut file_options = OpenOptions::new();
        file_options.read(true);
        file_options.write(false);

        let mut dev_file = file_options.open(format!("/dev/input/{}", event_filename)).unwrap();
        PointerEventsReader { dev_file }
    }
    pub fn next(&mut self) -> PointerEvent {
        let mut packet = [0u8; 16];
        self.dev_file.read_exact(&mut packet).unwrap();

        let mut rdr = Cursor::new(packet);
        let ts = rdr.read_u64::<NativeEndian>().unwrap();
        let e_type = rdr.read_u16::<NativeEndian>().unwrap();
        let e_code = rdr.read_u16::<NativeEndian>().unwrap();
        let e_value = rdr.read_u32::<NativeEndian>().unwrap();

        PointerEvent { ts, e_type, e_code, e_value }
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
    reader: Arc<RwLock<PointerEventsReader>>,
    x: Arc<RwLock<u32>>,
    y: Arc<RwLock<u32>>,
    viewport_width: u32,
    viewport_height: u32,
    pub rx: Arc<RwLock<Receiver<CapturedPointerEvent>>>
}

impl PointerEventsKeeper {
    pub fn new(event_filename: &str, viewport_width: u32, viewport_height: u32) -> PointerEventsKeeper {
        let reader = Arc::new(RwLock::new(PointerEventsReader::new(event_filename)));
        let x = Arc::new(RwLock::new(0));
        let y = Arc::new(RwLock::new(0));
        let (tx, rx) = mpsc::channel::<CapturedPointerEvent>();
        let rx = Arc::new(RwLock::new(rx));
        
        let reader_clone = reader.clone();
        let x_clone = x.clone();
        let y_clone = y.clone();
        let keeper = PointerEventsKeeper { reader, x, y, viewport_width, viewport_height, rx };
        thread::spawn(move || {
            let mut reader_lock = reader_clone.write().unwrap();
            loop {
                let e = reader_lock.next();
                let mut x_borrow = x_clone.write().unwrap();
                let mut y_borrow = y_clone.write().unwrap();
                match e.e_code {
                    53 => {
                        *x_borrow = ((e.e_value as f64 / 4096.0) * viewport_width as f64).round() as u32;
                    },
                    54 => {
                        *y_borrow = ((e.e_value as f64 / 4096.0) * viewport_height as f64).round() as u32;
                    },
                    57 => {
                        if e.e_value == 0xffffffff {
                            tx.send(CapturedPointerEvent::PointerOn(Coords { x: *x_borrow, y: *y_borrow }));
                        } else if e.e_value == 0x0 {
                            tx.send(CapturedPointerEvent::PointerOff(Coords { x: *x_borrow, y: *y_borrow }));
                        }
                    },
                    _ => {
                        println!("unknown event values {} {} {}", e.e_type, e.e_code, e.e_value)
                    }
                }
            }
        });

        keeper
    }
    pub fn coords(&self) -> Coords {
        let x_borrow = self.x.read().unwrap();
        let y_borrow = self.y.read().unwrap();
        Coords { x: *x_borrow, y: *y_borrow }
    }
}
