mod toc;
mod pointer_events;
mod process_runner;
mod logger;

use process_runner::quick_run;
use walkdir::{WalkDir};
use id3::{Tag, TagLike, frame::PictureType};
use pointer_events::{PointerEventsReader, PointerEventsKeeper, CapturedPointerEvent, Coords};
use std::collections::HashMap;
use std::io::{Write, BufReader, Read};
use std::ops::{Add, Sub};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Stdio, exit, ChildStdin, ChildStdout};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread::{self, current, sleep};
use std::sync::mpsc::{self, Sender, Receiver};
use std::time::{Duration, Instant};
use std::{process::{Command, Output}, os::unix::prelude::CommandExt};
use std::{str, vec, fmt};
use std::fs::{self, OpenOptions};
use std::io::BufRead;

use crate::process_runner::quick_write;

#[derive(Clone)]
pub struct Track {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    track: u32,
    year: i32,
    tag: Option<Tag>
}

#[derive(Clone)]
enum ControlMsg {

    PAUSE(),
    SEEK_FORWARD(),
    SEEK_BACKWARD(),
    NEXT(),
    PREV(),
    SETVOL(u32),
    SETTRACK(u32),
    GETVOL(),
    GETCURRENTTRACK(),
    GETCURRENTTRACKLENGTH(),
    GETTRACKINFO(u32),

    CURRENTTRACK(u32),
    NEWTRACK(u32),
    TRACKINFO(Track),
    VOL(u32),
    LENGTH(f32),
    POS(f32),
    PAUSED(bool)

}

fn get_num_from_process<T: Add<Output=T> + Sub<Output=T> + FromStr, F: Fn(String) -> String>(stdout: &mut BufReader<ChildStdout>, process_string: F) -> Option<T> {
    let mut v: Option<T> = None;
    loop {
        let mut l = String::new();
        if let Ok(a) = stdout.read_line(&mut l) {
            if a == 0 {
                return None;
            }
        }
        l = l.replace("\n", "");
        l = process_string(l);
        if let Ok(a) = l.parse::<T>() {
            v = Some(a);
            break;
        }
    }
    v
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name()
         .to_str()
         .map(|s| s.starts_with("."))
         .unwrap_or(false)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    //logger test (will exit)
    //logger::test();

    //check catalog
    log!("main", "reading tracks...");
    let mut albums: HashMap<String, Vec<Track>> = HashMap::new();
    let mut albums_order: Vec<(String, i32, String)> = Vec::new();

    log!("main", "opening /mnt/us/music...");
    quick_write(1, "* Cataloging...");
    for entry in WalkDir::new("/mnt/us/music").into_iter().filter_entry(|e| !is_hidden(e)) {
        let entry = result!(entry);
        match entry.path().extension() {
            None => continue,
            Some(ext) => if ext != "mp3" && ext != "wav" { continue; }
        }
        //default values in case tag is not available
        let mut title = entry.path().file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or("".into());
        let mut artist = String::new();
        let mut album = entry.path().parent().unwrap().file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or("".into());
        let mut track: u32 = 0;
        let mut year: i32 = 0;
        //read tag
        let tag_result = Tag::read_from_path(entry.path());
        let mut tag_to_store: Option<Tag> = None;
        if let Ok(tag) = tag_result {
            if let Some(id3artist) = tag.artist() {
                artist = String::from(id3artist);
            }
            if let Some(id3title) = tag.title() {
                title = String::from(id3title);
            }
            if let Some(id3album) = tag.album() {
                album = String::from(id3album);
            }
            if let Some(id3track) = tag.track() {
                track = id3track;
            }
            if let Some(id3year) = tag.year() {
                year = id3year;
            }
            if let Some(a) = tag.get("TDOR") {
                year = a.content().text().unwrap().parse::<i32>().unwrap();
            } else if let Some(a) = tag.get("TORY") {
                year = a.content().text().unwrap().parse::<i32>().unwrap();
            }
            tag_to_store = Some(tag);
        }
        if !albums.contains_key(&album) {
            albums.insert(album.clone(), Vec::new());
        }
        albums_order.push((artist.clone(), year, album.clone()));
        albums.get_mut(&album).unwrap().push(Track { path: entry.into_path(), title, artist, album, track, year, tag: tag_to_store });
    }

    log!("main", "sorting...");
    // sort catalog
    albums_order.sort_by(|a, b| {
        if a.0.eq_ignore_ascii_case(&b.0) {
            b.1.partial_cmp(&a.1).unwrap()
        } else {
            a.0.partial_cmp(&b.0).unwrap()
        }
    });

    let mut tracks: Vec<Track> = Vec::new();

    for a in albums_order {
        let album = albums.get_mut(&a.2).unwrap();
        album.sort_by(|a, b| a.track.partial_cmp(&b.track).unwrap());
        tracks.append(album);
    }

    // for now, print catalog
    /*for t in tracks.iter() {
        println!("{}/{} {} - {}", t.album, t.track, t.artist, t.title)
    }*/

    log!("main", "starting T.O.C. generation...");
    // generate T.O.C. pdf
    toc::gentoc(&tracks);

    log!("main", "spawning player control thread...");
    // spawn player control thread
    let (tx, rx) = mpsc::channel::<ControlMsg>();
    let (reply_tx, reply_rx) = mpsc::channel::<ControlMsg>();
    thread::spawn(move || {
        log!("player-control", "");
        let mut first_play = true;
        let mut current_volume: u32 = 60;
        let mut spawn_mplayer = |i: u32, current_volume: u32| {
            first_play = true;
            let mut child = result!(Command::new("/mnt/us/buck/bin/mplayer").args(vec![
                "-slave", "-quiet", "-demuxer", "35", "-volume", &current_volume.to_string(), "-softvol", "-softvol-max", "190", &tracks[i as usize].path.to_string_lossy().to_string()
            ]).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn());
            let mut stdin = child.stdin.take().unwrap();
            reply_tx.send(ControlMsg::NEWTRACK(i));
            let mut stdout = BufReader::new(child.stdout.take().unwrap());
            stdin.write_all(b"get_time_length\n");
            let mut length_of_song: f32 = get_num_from_process(&mut stdout, |s| s.replace("ANS_LENGTH=", "")).unwrap();
            reply_tx.send(ControlMsg::LENGTH(length_of_song));
            (child, stdin, stdout, length_of_song)
        };
        let mut currently_playing: u32 = 0;
        let mut currently_paused: bool = false;
        let mut set_currently_paused = |currently_paused: &mut bool, v: bool| {
            *currently_paused = v;
            reply_tx.send(ControlMsg::PAUSED(v));
        };
        set_currently_paused(&mut currently_paused, false);
        let (mut child, mut stdin, mut stdout, mut length_of_song) = spawn_mplayer(currently_playing, current_volume);
        set_currently_paused(&mut currently_paused, true);
        stdin.write_all(b"pause\n"); //start paused by default

        log!("player-control", "entering event loop...");
        loop {
            // check for control messages
            if let Ok(m) = rx.recv_timeout(Duration::from_millis(100)) {
                match m {
                    ControlMsg::PAUSE() => {
                        let new_pause_state = !currently_paused;
                        set_currently_paused(&mut currently_paused, new_pause_state);
                        log!("player-control", "mplayer: {}", "pause");
                        stdin.write_all(b"pause\n");
                    },
                    ControlMsg::SEEK_FORWARD() => {
                        set_currently_paused(&mut currently_paused, false);
                        log!("player-control", "mplayer: {}", "seek");
                        stdin.write_all(b"seek 5 0\n");
                    },
                    ControlMsg::SEEK_BACKWARD() => {
                        set_currently_paused(&mut currently_paused, false);
                        log!("player-control", "mplayer: {}", "seek");
                        stdin.write_all(b"seek -5 0\n");
                    },
                    ControlMsg::NEXT() => {
                        child.kill();
                        currently_playing += 1;
                        if currently_playing as usize >= tracks.len() { currently_playing = 0; }
                        log!("player-control", "-next- removing old player, currently playing is now {}", currently_playing);
                        let tmp = spawn_mplayer(currently_playing, current_volume);
                        child = tmp.0;
                        stdin = tmp.1;
                        stdout = tmp.2;
                        length_of_song = tmp.3;
                    },
                    ControlMsg::PREV() => {
                        child.kill();
                        if currently_playing == 0 { currently_playing = (tracks.len() - 1) as u32; }
                        else { currently_playing -= 1; }
                        log!("player-control", "-prev- removing old player, currently playing is now {}", currently_playing);
                        let tmp = spawn_mplayer(currently_playing, current_volume);
                        child = tmp.0;
                        stdin = tmp.1;
                        stdout = tmp.2;
                        length_of_song = tmp.3;
                    },
                    ControlMsg::SETVOL(v) => {
                        log!("player-control", "mplayer: volume {}", v);
                        current_volume = v;
                        set_currently_paused(&mut currently_paused, false);
                        stdin.write_all(format!("volume {} 1\n", v.to_string()).as_bytes());
                        reply_tx.send(ControlMsg::VOL(current_volume));
                    },
                    ControlMsg::SETTRACK(t) => {
                        if t < tracks.len() as u32 && t >= 0 {
                            log!("player-control", "-set- removing old player, currently playing is now {}", currently_playing);
                            child.kill();
                            currently_playing = t;
                            let tmp = spawn_mplayer(currently_playing, current_volume);
                            child = tmp.0;
                            stdin = tmp.1;
                            stdout = tmp.2;
                            length_of_song = tmp.3;
                        } else {
                            log!("player-control", "-set- track number received is out of range ({})! ignoring..", t);
                            println!("out of range");
                        }
                    },
                    ControlMsg::GETVOL() => {
                        log!("player-control", "getvol");
                        reply_tx.send(ControlMsg::VOL(current_volume));
                    },
                    ControlMsg::GETCURRENTTRACK() => {
                        log!("player-control", "getcurrenttrack");
                        reply_tx.send(ControlMsg::CURRENTTRACK(currently_playing));
                    },
                    ControlMsg::GETCURRENTTRACKLENGTH() => {
                        log!("player-control", "getcurrenttracklength");
                        reply_tx.send(ControlMsg::LENGTH(length_of_song));
                    },
                    ControlMsg::GETTRACKINFO(t) => {
                        log!("player-control", "gettrackinfo");
                        reply_tx.send(ControlMsg::TRACKINFO(tracks[t as usize].clone()));
                    },
                    _ => {}
                }
            }
            // check if song has finished playing
            if let Some(exit_status) = result!(child.try_wait()) {
                currently_playing += 1;
                if currently_playing >= tracks.len() as u32 { currently_playing = 0; }
                log!("player-control", "yes! moving to next track {}", currently_playing);
                let tmp = spawn_mplayer(currently_playing, current_volume);
                child = tmp.0;
                stdin = tmp.1;
                stdout = tmp.2;
                length_of_song = tmp.3;
            }
            // check song current play position
            if !currently_paused {
                let result = stdin.write_all(b"get_time_pos\n");
                let mut time_pos_result = get_num_from_process(&mut stdout, |s| s.replace("ANS_TIME_POSITION=", ""));
                if let Some(time_pos) = time_pos_result {
                    reply_tx.send(ControlMsg::POS(time_pos));
                }
            }
            // repeat
        }
    });

    // event manager has 'static lifetime, must exist until the end of the program
    log!("main", "booting up the events manager...");
    let mut event_manager = PointerEventsKeeper::new("event3", 600, 800);
    log!("main", "giving control to ui...");
    ui(&tx, &reply_rx, event_manager.rx.clone());

    Ok(())

}

fn draw_album_art(path: &str) {
    quick_run("fbink", vec!["-g", &format!("file={},w=-1,dither", path)]);
}

fn draw_text(text: &str, size: u32, top: u32, left: u32, style: &str, bg_color: &str, fg_color: &str) {
    let options = format!("size={},top={},left={},style={},regular=/mnt/us/buck/assets/Bookerly-Regular.ttf,bold=/mnt/us/buck/assets/Bookerly-Bold.ttf,italic=/mnt/us/buck/assets/Bookerly-Italic.ttf,bolditalic=/mnt/us/buck/assets/Bookerly-BoldItalic.ttf", size, top, left, style);
    quick_run("fbink", vec!["-t", &options, "-B", bg_color, "-C", fg_color, "--bgless", text]);
}

fn draw_text_with_bg(text: &str, size: u32, top: u32, left: u32, font: &str, bg_color: &str, fg_color: &str) {
    let options = format!("size={},top={},left={},regular={}", size, top, left, &font);
    quick_run("fbink", vec!["-t", &options, "-C", fg_color, "-B", bg_color, text]);
}

struct Elapsed {
    last: Instant
}
impl Elapsed {
    pub fn new() -> Elapsed {
        Elapsed { last: Instant::now() }
    }
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.last)
    }
    pub fn update(&mut self) {
        self.last = Instant::now();
    }
}

// x_start, x_end, y_start, y_end, content, font_size
struct BoundingBoxTextInteractive(u32, u32, u32, u32, String, u32, Elapsed);
impl BoundingBoxTextInteractive {
    fn draw(&self) {
        draw_text(&self.4, self.5, self.2, self.0, "regular", "black", "white");
    }
    fn draw_over(&self) {
        clear_canvas_partly("black", self.2, self.0, self.1-self.0, self.3-self.2);
    }
    fn draw_dbg(&self) {
        clear_canvas_partly("WHITE", self.2, self.0, self.1-self.0, self.3-self.2);
    }
    fn colliding(&self, x: u32, y: u32) -> bool {
        x >= self.0 && x <= self.1 && y >= self.2 && y <= self.3
    }
    fn colliding_coords(&mut self, coords: &Coords) -> bool {
        if self.6.elapsed() < Duration::from_millis(200) {
            false
        } else {
            self.6.update();
            self.colliding(coords.x as u32, coords.y as u32)
        }
    }
    fn local_coords(&self, x: u32, y: u32) -> Coords {
        Coords { x: (x - self.0) as u32, y: (y - self.2) as u32 }
    }
}

fn draw_song(track: &Track, skip_album_art: bool) {
    sleep(Duration::from_millis(1000));
    if skip_album_art {
        clear_canvas_partly("BLACK", 600, 0, 600, 200);
    } else {
        clear_canvas("BLACK");
    }
    clear_canvas_partly("GRAY6", 600, 0, 600, 10);
    if !skip_album_art {
        let mut has_album_cover = false;
        if let Some(tag) = &track.tag {
            for p in tag.pictures() {
                if p.picture_type == PictureType::CoverFront {
                    has_album_cover = true;
                    let mut file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .open("/tmp/bucktempalbumstore").unwrap();
                    let result = file.write_all(&p.data);
                    if let Err(e) = result {
                        println!("{}", &e);
                    }
                }
            }
        }
        if has_album_cover {
            draw_album_art("/tmp/bucktempalbumstore");
        } else {
            draw_album_art("/mnt/us/buck/assets/no-album-cover.jpg")
        }
    }
    draw_text(&track.title, 17, 710, 10, "regular", "black", "white");
    draw_text(&track.artist, 12, 756, 10, "italic", "black", "white");
}

fn draw_all(t: &Track, controls: Vec<&BoundingBoxTextInteractive>, current_album_is_new: &mut bool) {
    draw_song(t, !*current_album_is_new);
    for c in controls {
        c.draw();
    }
    if *current_album_is_new {
        *current_album_is_new = false;
    }
}

fn draw_two_state(b: &bool, on: &BoundingBoxTextInteractive, off: &BoundingBoxTextInteractive) {
    if *b {
        off.draw_over();
        on.draw();
    } else {
        on.draw_over();
        off.draw();
    }
}

fn clear_canvas(color: &str) {
    quick_run("fbink", vec!["--cls", &format!("--background={}", &color)]);
}

fn clear_canvas_partly(color: &str, top: u32, left: u32, width: u32, height: u32) {
    quick_run("fbink", vec!["--cls", &format!("top={},left={},width={},height={}", top, left, width, height), &format!("--background={}", &color)]);
}

fn ui(sender: &Sender<ControlMsg>, receiver: &Receiver<ControlMsg>, event_rx: Arc<RwLock<Receiver<CapturedPointerEvent>>>) {
    log!("ui", "visible is false");
    let mut visible: bool = false;

    // setup socket
    fs::remove_file("/tmp/buck.sock");
    let listener = match UnixListener::bind("/tmp/buck.sock") {
        Ok(sock) => sock,
        Err(e) => {
            log!("main", &format!("couldn't connect to pipe: {:?}", e));
            exit(0);
        }
    };
    listener.set_nonblocking(true);

    let mut width: u32 = 600;
    let mut height: u32 = 800;

    sender.send(ControlMsg::GETCURRENTTRACK());
    sender.send(ControlMsg::GETCURRENTTRACKLENGTH());
    sender.send(ControlMsg::GETVOL());
    let mut current_track_length: f32 = -1.0;
    let mut last_progress_chunk_leftpad: f32 = 0.0;
    let mut current_pos: f32 = 0.0;
    let mut accum_pos: f32 = 0.0;

    // buttons
    let FORWARD_BACKWARD_BTN_PAD = 30;
    let PREV_NEXT_BTN_LR_PAD = 10;
    let PAD_FROM_COVER = 35;
    let PAD_FROM_COVER_ABS = PAD_FROM_COVER + 600;
    let mut prev = BoundingBoxTextInteractive(PREV_NEXT_BTN_LR_PAD, PREV_NEXT_BTN_LR_PAD + 100, PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + 40, String::from("Previous"), 12, Elapsed::new());
    
    let back5sleft = (600/2)-9-52-FORWARD_BACKWARD_BTN_PAD;
    let mut back5s = BoundingBoxTextInteractive(back5sleft, back5sleft + 40, PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + 40, String::from("< 5s"), 12, Elapsed::new());
    
    let playleft = (600/2)-9;
    let mut play = BoundingBoxTextInteractive(playleft, playleft + 40, PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + 40, String::from("▶"), 15, Elapsed::new());
    let mut pause = BoundingBoxTextInteractive(playleft, playleft + 40, PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + 40, String::from("| |"), 12, Elapsed::new());
    
    let forward5sleft = (600/2)-9+20+FORWARD_BACKWARD_BTN_PAD;
    let mut forward5s = BoundingBoxTextInteractive(forward5sleft, forward5sleft + 40, PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + 40, String::from("5s >"), 12, Elapsed::new());
    
    let nextleft = 600 - 60;
    let mut next = BoundingBoxTextInteractive(nextleft, nextleft + 100, PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + 40, String::from("Next"), 12, Elapsed::new());
    
    let closeleft = 600-10-14-50;
    let closetop = 800-10-14-50;
    let mut close = BoundingBoxTextInteractive(closeleft, 600, closetop, 800, String::from("✕"), 20, Elapsed::new());

    let mut volume_control = BoundingBoxTextInteractive(0, width, 0, height/2, String::new(), 1, Elapsed::new());

    let mut current_track: Option<Track> = None;
    let mut current_album_is_new: bool = true;
    let mut current_album: String = String::new();
    let mut currently_paused: bool = true;

    'eventloop: loop {
        // process new pointer events
        let mut e = event_rx.read().unwrap().recv_timeout(Duration::from_millis(50));
        while let Ok(pointer_evt) = e {
            // only process these events if we have reign over the ui
            if visible {
                match pointer_evt {
                    CapturedPointerEvent::PointerOn(coords) => {
                        if play.colliding_coords(&coords) || pause.colliding_coords(&coords) {
                            sender.send(ControlMsg::PAUSE());
                        } else if back5s.colliding_coords(&coords) {
                            sender.send(ControlMsg::SEEK_BACKWARD());
                        } else if forward5s.colliding_coords(&coords) {
                            sender.send(ControlMsg::SEEK_FORWARD());
                        } else if prev.colliding_coords(&coords) {
                            sender.send(ControlMsg::PREV());
                        } else if next.colliding_coords(&coords) {
                            sender.send(ControlMsg::NEXT());
                        } else if close.colliding_coords(&coords) {
                            quick_run("sh", vec!["/mnt/us/buck/bin/enable-touch.sh"]);
                            visible = false;
                        } else if volume_control.colliding_coords(&coords) {
                            let mut x = volume_control.local_coords(coords.x, coords.y).x as f32;
                            let lr_minmax_margin = width as f32 / 6.0;
                            let variable_margin = lr_minmax_margin * 4.0;
                            let right_edge = lr_minmax_margin + variable_margin;
                            if x <= lr_minmax_margin {
                                x = lr_minmax_margin;
                            } else if x >= right_edge {
                                x = right_edge;
                            }
                            let new_vol = ((x - lr_minmax_margin) * 100.0 / variable_margin).round() as u32;
                            sender.send(ControlMsg::SETVOL(new_vol));
                        }
                    },
                    CapturedPointerEvent::PointerOff(coords) => {
                    }
                }
            }
            e = event_rx.read().unwrap().recv_timeout(Duration::from_millis(50));
        }
        let mut e = receiver.recv_timeout(Duration::from_millis(50));

        // filter out duplicate typed control messages
        let mut control_messages: Vec<ControlMsg> = Vec::new();
        while let Ok(msg) = e {
            control_messages.push(msg);
            e = receiver.recv_timeout(Duration::from_millis(50));
        }
        let mut last_currenttrack_or_newtrack: Option<ControlMsg> = None;
        let mut last_length: Option<ControlMsg> = None;
        let mut last_pos: Option<ControlMsg> = None;
        let mut last_paused: Option<ControlMsg> = None;
        control_messages.retain(|c| {
            match &c {
                ControlMsg::CURRENTTRACK(_) => {
                    last_currenttrack_or_newtrack = Some(c.clone());
                    false
                },
                ControlMsg::NEWTRACK(_) => {
                    last_currenttrack_or_newtrack = Some(c.clone());
                    false
                },
                ControlMsg::LENGTH(_) => {
                    last_length = Some(c.clone());
                    false
                },
                ControlMsg::POS(_) => {
                    last_pos = Some(c.clone());
                    false
                },
                ControlMsg::PAUSED(_) => {
                    last_paused = Some(c.clone());
                    false
                },
                _ => true
            }
        });
        if let Some(v) = last_currenttrack_or_newtrack { control_messages.push(v); }
        if let Some(v) = last_length { control_messages.push(v); }
        if let Some(v) = last_pos { control_messages.push(v); }
        if let Some(v) = last_paused { control_messages.push(v); }
        // process control messages
        for msg in control_messages {
            match msg {
                ControlMsg::CURRENTTRACK(currently_playing) => {
                    sender.send(ControlMsg::GETTRACKINFO(currently_playing));
                },
                ControlMsg::NEWTRACK(currently_playing) => {
                    sender.send(ControlMsg::GETTRACKINFO(currently_playing));
                },
                ControlMsg::TRACKINFO(track) => {
                    current_track = Some(track);
                    if !current_album_is_new {
                        if current_track.as_ref().unwrap().album != current_album {
                            current_album_is_new = true;
                        }
                    }
                    current_album = current_track.as_ref().unwrap().album.clone();
                    if visible {
                        draw_all(current_track.as_ref().unwrap(), vec![&mut prev,
                                                &mut back5s,
                                                &mut pause,
                                                &mut forward5s,
                                                &mut next,
                                                &mut close], &mut current_album_is_new);
                    }
                },
                ControlMsg::LENGTH(length) => {
                    current_track_length = length;
                },
                ControlMsg::POS(pos) => {
                    if current_track_length != -1.0 {
                        let width_per_progress: u32 = 6;
                        accum_pos += pos - current_pos;
                        if (accum_pos / current_track_length).abs() >= (width_per_progress as f32/width as f32) {
                            let diff_width = ((accum_pos / current_track_length) * (width as f32/width_per_progress as f32)).floor() as i64 * width_per_progress as i64;
                            let diff_width = diff_width.max(-(width as i64));
                            let progress_chunk_leftpad = (((pos / current_track_length) * (width as f32/width_per_progress as f32)).floor() as i64 * width_per_progress as i64) as f32;
                            //let progress_chunk_leftpad = (last_progress_chunk_leftpad + diff_width as f32).min(width as f32);
                            let mut start = last_progress_chunk_leftpad as i64;
                            let mut end = progress_chunk_leftpad as i64;
                            let mut color = "GRAYD";
                            if end < start {
                                color = "GRAY6";
                                let tmp = end;
                                end = start+1; //account for flooring
                                start = tmp;
                            }
                            if visible { clear_canvas_partly(color, 600, start as u32, (end - start) as u32, 10); }
                            last_progress_chunk_leftpad = progress_chunk_leftpad;
                            let accum_pos_sign = accum_pos.abs() / accum_pos;
                            accum_pos = ((accum_pos / current_track_length).abs() % (width_per_progress as f32/width as f32)) * accum_pos_sign;
                            accum_pos *= current_track_length;
                        }
                        current_pos = pos;
                    }
                },
                ControlMsg::PAUSED(b) => {
                    currently_paused = b;
                    if visible {
                        draw_two_state(&b, &play, &pause);
                    }
                },
                ControlMsg::VOL(new_vol) => {
                    if visible {
                        draw_text_with_bg(&format!(" Volume {: >3} ", new_vol.to_string()), 9, 600-50, 2, "/mnt/us/buck/assets/LinLibertine_M.otf", "BLACK", "WHITE");
                    }
                },
                _ => {}
            }
        }

        // process new clients on socket
        match listener.accept() {
            Ok((mut socket, addr)) => {
                let mut cmd = String::new();
                socket.read_to_string(&mut cmd);
                if cmd.starts_with("select") {
                    let t = cmd.replace("select ", "").parse::<u32>().unwrap();
                    sender.send(ControlMsg::SETTRACK(t-1));
                } else if cmd.starts_with("ui") {
                    if let Some(current_track) = &current_track {
                        quick_run("sh", vec!["/mnt/us/buck/bin/disable-touch.sh"]);
                        visible = true;
                        draw_all(current_track, vec![&mut prev,
                                                &mut back5s,
                                                &mut pause,
                                                &mut forward5s,
                                                &mut next,
                                                &mut close], &mut true);
                        draw_two_state(&currently_paused, &play, &pause);
                        clear_canvas_partly("GRAYD", 600, 0, last_progress_chunk_leftpad as u32, 10);
                    }
                }
            },
            Err(e) => {},
        }
    }
}
