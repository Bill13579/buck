mod toc;
mod read_config;
mod pointer_events;
mod process_runner;
mod logger;
mod btctl_keepalive;
mod utils;

use process_runner::quick_run;
use walkdir::{WalkDir};
use id3::{Tag, TagLike, frame::PictureType};
use pointer_events::{PointerEventsReader, PointerEventsKeeper, CapturedPointerEvent, Coords};
use std::collections::HashMap;
use std::io::{Write, BufReader, Read};
use std::ops::{Add, Sub};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Stdio, exit, ChildStdin, ChildStdout, Child};
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::thread::{self, current, sleep};
use std::sync::mpsc::{self, Sender, Receiver};
use std::time::{Duration, Instant};
use std::{process::{Command, Output}, os::unix::prelude::CommandExt};
use std::{str, vec, fmt};
use std::fs::{self, OpenOptions};
use std::io::BufRead;
use utils::elapsed::Elapsed;

use crate::read_config::root;
use crate::process_runner::quick_write;

#[derive(Clone)]
pub struct Track {
    path: PathBuf,
    title: String,
    artist: String,
    album: String,
    track: u32,
    disc: u32,
    year: i32,
    album_artist: String,
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
    UIHIDDEN(),
    UIOPENED(),

    CURRENTTRACK(u32),
    NEWTRACK(u32),
    TRACKINFO(Track),
    VOL(u32),
    LENGTH(f32),
    POS(f32),
    PAUSED(bool)

}

fn get_num_from_process<T: Add<Output=T> + Sub<Output=T> + FromStr, F: Fn(String) -> String>(stdout: &mut BufReader<ChildStdout>, process_string: F, goal: T) -> Option<T> {
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

fn check_output_for_or_exited<F: Fn(String) -> bool>(stdout: &mut BufReader<ChildStdout>, process_string: F) -> bool {
    loop {
        let mut l = String::new();
        if let Ok(a) = stdout.read_line(&mut l) {
            if a == 0 {
                return false;
            }
        }
        l = l.replace("\n", "");
        if process_string(l) { return true; }
    }
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.file_name()
         .to_str()
         .map(|s| s.starts_with("."))
         .unwrap_or(false)
}

fn kill_and_wait(child: &mut Child) {
    if let Ok(_) = child.kill() {
        sleep(Duration::from_millis(10));
    }
    sleep(Duration::from_millis(100));
}

struct Album {
    artists: HashMap<String, usize>,
    tracks: Vec<Track>
}
impl Album {
    pub fn new() -> Album {
        Album { artists: HashMap::new(), tracks: Vec::new() }
    }
    pub fn push(&mut self, t: Track) {
        *self.artists.entry(t.artist.clone()).or_insert(0) += 1;
        self.tracks.push(t);
    }
    pub fn tracks(&mut self) -> &mut Vec<Track> {
        &mut self.tracks
    }
    pub fn artist(&self) -> String {
        if let Some(a) = self.artists.iter().max_by(|x, y| x.1.cmp(y.1)) {
            a.0.clone()
        } else {
            String::new()
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {

    //logger test (will exit)
    //logger::test();

    //reading config
    let config = read_config::read_config();

    //check catalog
    log!("main", "reading tracks...");
    let mut albums: HashMap<String, Album> = HashMap::new();
    let mut albums_order: Vec<(String, i32, String)> = Vec::new();

    log!("main", "opening music directories...");
    quick_write(1, "* Cataloging...");
    for music_dir in &config.music_dirs {
        for entry in WalkDir::new(music_dir).into_iter().filter_entry(|e| !is_hidden(e)) {
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
            let mut disc: u32 = 1;
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
                if let Some(id3disc) = tag.disc() {
                    disc = id3disc;
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
                albums.insert(album.clone(), Album::new());
            }
            albums_order.push((artist.clone(), year, album.clone()));
            albums.get_mut(&album).unwrap().push(Track { path: entry.into_path(), title, artist, album, track, disc, year, album_artist: String::new(), tag: tag_to_store });
        }
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
        album.tracks().sort_by(|a, b| {
            if a.disc == b.disc {
                a.track.partial_cmp(&b.track).unwrap()
            } else {
                a.disc.partial_cmp(&b.disc).unwrap()
            }
        });
        let album_artist = album.artist();
        if let Some(first_track) = album.tracks().get_mut(0) {
            first_track.album_artist = album_artist;
        }
        tracks.append(album.tracks());
    }

    // for now, print catalog
    /*for t in tracks.iter() {
        println!("{}/{} {} - {}", t.album, t.track, t.artist, t.title)
    }*/

    if tracks.len() == 0 {
        quick_write(2, "* Music directory is empty, nothing to play");
        quick_write(3, "   Exiting");
        exit(0);
    }

    log!("main", "starting T.O.C. generation...");
    // generate T.O.C. pdf
    toc::gentoc(&tracks, PathBuf::from(&config.documents_dir).join("Buck - Table of Contents.pdf"));

    log!("main", "spawning player control thread...");
    // spawn player control thread
    let (tx, rx) = mpsc::channel::<ControlMsg>();
    let (reply_tx, reply_rx) = mpsc::channel::<ControlMsg>();
    thread::spawn(move || {
        log!("player-control", "");
        let mut btonly_keepalive: Option<btctl_keepalive::BTKeepAlive> = None;
        let mut last_time_pos: f32 = 0.0;
        let mut first_play = true;
        let mut current_volume: u32 = 60;
        let mut spawn_mplayer_base = |i: u32, current_volume: u32| {
            first_play = true;
            let current_volume_str = current_volume.to_string();
            let track_path_str = tracks[i as usize].path.to_string_lossy().to_string();
            let mut child_args = vec![
                "-slave", "-quiet", "-volume", &current_volume_str, "-softvol", "-softvol-max", "110", &track_path_str
            ];
            if cfg!(feature = "kindle") {
                child_args.insert(2, "35");
                child_args.insert(2, "-demuxer");
            }
            if cfg!(feature = "btonly") {
                child_args.insert(0, "alsa:device=bluealsa");
                child_args.insert(0, "-ao");
            }
            let mut child = result!(Command::new(root("bin/mplayer")).args(child_args).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn());
            let mut stdin;
            let mut stdout;
            match child.stdin.take() {
                Some(cstdin) => {
                    stdin = cstdin;
                },
                None => { return None; }
            }
            match child.stdout.take() {
                Some(cstdout) => {
                    stdout = BufReader::new(cstdout);
                },
                None => { return None; }
            }
            if !check_output_for_or_exited(&mut stdout, |s: String| s.contains("AO: [alsa]")) {
                child.wait();
                return None;
            }
            Some((child, stdin, stdout))
        };
        let mut spawn_mplayer = |i: u32, current_volume: u32, btonly_keepalive: &mut Option<btctl_keepalive::BTKeepAlive>| {
            let mut child;
            let mut stdin;
            let mut stdout;
            quick_write(8, "* Spawning mplayer");
            let mut attmpt_binary: bool = false;
            loop {
                let mplayer_result = spawn_mplayer_base(i, current_volume);
                if let Some(mplayer) = mplayer_result {
                    child = mplayer.0;
                    stdin = mplayer.1;
                    stdout = mplayer.2;
                    break;
                }
                if attmpt_binary == false { attmpt_binary = true; }
                else { attmpt_binary = false; }
                quick_write(8, &format!("{} Mplayer spawn failed! Retrying after 5 seconds", if attmpt_binary { "|" } else { "=" }));
                quick_write(9, "   (are you connected to a Bluetooth speaker?)");
                sleep(Duration::from_secs(5));
            }
            // handle Bluetooth keep-alive for btonly devices
            if cfg!(feature = "btonly") {
                *btonly_keepalive = Some(btctl_keepalive::BTKeepAlive::spawn());
            }
            // notify UI
            reply_tx.send(ControlMsg::NEWTRACK(i));
            stdin.write_all(b"get_time_length\n");
            let length_of_song = get_num_from_process(&mut stdout, |s| s.replace("ANS_LENGTH=", ""), 0.0f32).unwrap();
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
        let (mut child, mut stdin, mut stdout, mut length_of_song) = spawn_mplayer(currently_playing, current_volume, &mut btonly_keepalive);
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
                        if cfg!(feature = "btonly") {
                            if let Some(ka) = &mut btonly_keepalive { ka.scan_on_temp(); }
                        }
                        set_currently_paused(&mut currently_paused, false);
                        log!("player-control", "mplayer: {}", "seek");
                        stdin.write_all(b"seek 5 0\n");
                    },
                    ControlMsg::SEEK_BACKWARD() => {
                        if cfg!(feature = "btonly") {
                            if let Some(ka) = &mut btonly_keepalive { ka.scan_on_temp(); }
                        }
                        set_currently_paused(&mut currently_paused, false);
                        log!("player-control", "mplayer: {}", "seek");
                        stdin.write_all(b"seek -5 0\n");
                    },
                    ControlMsg::NEXT() => {
                        kill_and_wait(&mut child);
                        currently_playing += 1;
                        if currently_playing as usize >= tracks.len() { currently_playing = 0; }
                        log!("player-control", "-next- removing old player, currently playing is now {}", currently_playing);
                        let tmp = spawn_mplayer(currently_playing, current_volume, &mut btonly_keepalive);
                        child = tmp.0;
                        stdin = tmp.1;
                        stdout = tmp.2;
                        length_of_song = tmp.3;
                    },
                    ControlMsg::PREV() => {
                        kill_and_wait(&mut child);
                        if currently_playing == 0 { currently_playing = (tracks.len() - 1) as u32; }
                        else { currently_playing -= 1; }
                        log!("player-control", "-prev- removing old player, currently playing is now {}", currently_playing);
                        let tmp = spawn_mplayer(currently_playing, current_volume, &mut btonly_keepalive);
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
                            kill_and_wait(&mut child);
                            currently_playing = t;
                            let tmp = spawn_mplayer(currently_playing, current_volume, &mut btonly_keepalive);
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
                    ControlMsg::UIOPENED() => {
                        log!("player-control", "ui opened");
                        // handle Bluetooth keep-alive for btonly devices
                        if cfg!(feature = "btonly") {
                            btonly_keepalive = Some(btctl_keepalive::BTKeepAlive::spawn());
                            if currently_paused {
                                log!("player-control", "-uiopen,restart- removing old player, currently playing is now {}", currently_playing);
                                kill_and_wait(&mut child);
                                let tmp = spawn_mplayer(currently_playing, current_volume, &mut btonly_keepalive);
                                child = tmp.0;
                                stdin = tmp.1;
                                stdout = tmp.2;
                                length_of_song = tmp.3;
                                stdin.write_all(format!("seek {} 2\n", last_time_pos).as_bytes());
                                stdin.write_all(b"pause\n"); //resume paused state
                            }
                        }
                    },
                    ControlMsg::UIHIDDEN() => {
                        log!("player-control", "ui hidden");
                        // handle Bluetooth keep-alive for btonly devices
                        if cfg!(feature = "btonly") && currently_paused {
                            btonly_keepalive = None;
                        }
                    },
                    _ => {}
                }
            }
            // check if song has finished playing
            if let Some(exit_status) = result!(child.try_wait()) {
                currently_playing += 1;
                if currently_playing >= tracks.len() as u32 { currently_playing = 0; }
                log!("player-control", "yes! moving to next track {}", currently_playing);
                let tmp = spawn_mplayer(currently_playing, current_volume, &mut btonly_keepalive);
                child = tmp.0;
                stdin = tmp.1;
                stdout = tmp.2;
                length_of_song = tmp.3;
            }
            // check song current play position
            if !currently_paused {
                let result = stdin.write_all(b"get_time_pos\n");
                let mut time_pos_result = get_num_from_process(&mut stdout, |s| s.replace("ANS_TIME_POSITION=", ""), 0.0f32);
                if let Some(time_pos) = time_pos_result {
                    last_time_pos = time_pos;
                    reply_tx.send(ControlMsg::POS(time_pos));
                }
            }
            // repeat
        }
    });

    // event manager has 'static lifetime, must exist until the end of the program
    log!("main", "booting up the events manager...");
    let mut event_manager = PointerEventsKeeper::new(PathBuf::from(config.event_paths.pointer), config.ui.width, config.ui.height);
    println!("BBB1");
    event_manager.start_thread();
    println!("BBB2");
    log!("main", "giving control to ui...");
    ui(&tx, &reply_rx, event_manager, config.ui.width, config.ui.height, config.ui.scale, config.disable_scrub);

    Ok(())

}

fn draw_album_art(path: &str) {
    quick_run("fbink", vec!["-g", &format!("file={},w=-1,dither", path)]);
}

fn draw_text(text: &str, size: u32, top: u32, left: u32, style: &str, bg_color: &str, fg_color: &str) {
    let options = format!("size={},top={},left={},style={},regular={},bold={},italic={},bolditalic={}", size, top, left, style, root("assets/Bookerly-Regular.ttf").display().to_string(), root("assets/Bookerly-Bold.ttf").display().to_string(), root("assets/Bookerly-Italic.ttf").display().to_string(), root("assets/Bookerly-BoldItalic.ttf").display().to_string());
    quick_run("fbink", vec!["-t", &options, "-B", bg_color, "-C", fg_color, "--bgless", text]);
}

fn draw_text_with_bg(text: &str, size: u32, top: u32, left: u32, font: &str, bg_color: &str, fg_color: &str) {
    let options = format!("size={},top={},left={},regular={}", size, top, left, &font);
    quick_run("fbink", vec!["-t", &options, "-C", fg_color, "-B", bg_color, text]);
}

struct BoundingBoxTextInteractive {
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
    x_display_pad: u32,
    y_display_pad: u32,
    content: String,
    font_size: u32,
    bg_color: String,
    fg_color: String,
    elapsed: Elapsed,
    disabled: bool
}
impl BoundingBoxTextInteractive {
    fn new(x_start: u32, x_end: u32, y_start: u32, y_end: u32, x_display_pad: u32, y_display_pad: u32, content: String, font_size: u32, bg_color: String, fg_color: String, elapsed: Elapsed) -> BoundingBoxTextInteractive {
        BoundingBoxTextInteractive { x_start, x_end, y_start, y_end, x_display_pad, y_display_pad, content, font_size, bg_color, fg_color, elapsed, disabled: false }
    }
    fn disable(&mut self) {
        self.disabled = true;
    }
    fn draw(&self) {
        if !self.disabled {
            draw_text(&self.content, self.font_size, self.y_start + self.y_display_pad, self.x_start + self.x_display_pad, "regular", &self.bg_color, &self.fg_color);
        }
    }
    fn draw_over(&self) {
        clear_canvas_partly("black", self.y_start, self.x_start, self.x_end-self.x_start, self.y_end-self.y_start);
    }
    fn draw_dbg(&self) {
        clear_canvas_partly("WHITE", self.y_start, self.x_start, self.x_end-self.x_start, self.y_end-self.y_start);
    }
    fn colliding(&self, x: u32, y: u32) -> bool {
        if self.disabled {
            false
        } else {
            x >= self.x_start && x <= self.x_end && y >= self.y_start && y <= self.y_end
        }
    }
    fn colliding_coords(&mut self, coords: &Coords) -> bool {
        if self.elapsed.elapsed() < Duration::from_millis(200) {
            false
        } else {
            self.elapsed.update();
            self.colliding(coords.x as u32, coords.y as u32)
        }
    }
    fn local_coords(&self, x: u32, y: u32) -> Coords {
        Coords { x: (x - self.x_start) as u32, y: (y - self.y_start) as u32 }
    }
}

fn draw_song(track: &Track, skip_album_art: bool, width: u32, height: u32, scale: f32) {
    sleep(Duration::from_millis(1000));
    if skip_album_art {
        clear_canvas_partly("BLACK", width, 0, width, height - width);
    } else {
        clear_canvas("BLACK");
    }
    clear_canvas_partly("GRAY6", width, 0, width, 10);
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
            draw_album_art(&root("assets/no-album-cover.jpg").display().to_string());
        }
    }
    draw_text(&track.title, scale_calc(17, scale), width + scale_calc(110, scale) - scale_calc(5, scale), 10, "regular", "black", "white");
    draw_text(&track.artist, scale_calc(12, scale), width + scale_calc(156, scale) - scale_calc(5, scale), 10, "italic", "black", "white");
}

fn draw_all(t: &Track, controls: Vec<&BoundingBoxTextInteractive>, current_album_is_new: &mut bool, width: u32, height: u32, scale: f32) {
    draw_song(t, !*current_album_is_new, width, height, scale);
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
    quick_run("fbink", vec!["--cls", &format!("--background={}", &color), "--wait"]);
}

fn clear_canvas_partly(color: &str, top: u32, left: u32, width: u32, height: u32) {
    quick_run("fbink", vec!["--cls", &format!("top={},left={},width={},height={}", top, left, width, height), &format!("--background={}", &color), "--wait"]);
}

fn rem_last(value: &str) -> &str {
    let mut chars = value.chars();
    chars.next_back();
    chars.as_str()
}

fn scale_calc(v: u32, scale: f32) -> u32 {
    (v as f32 * scale).round() as u32
}

fn ui(sender: &Sender<ControlMsg>, receiver: &Receiver<ControlMsg>, mut events_keeper: PointerEventsKeeper, width: u32, height: u32, scale: f32, disable_scrub: bool) {
    log!("ui", "visible is false");
    let mut player_visible: bool = false;
    let mut selector_visible: bool = false;

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

    sender.send(ControlMsg::GETCURRENTTRACK());
    sender.send(ControlMsg::GETCURRENTTRACKLENGTH());
    sender.send(ControlMsg::GETVOL());
    let mut current_track_length: f32 = -1.0;
    let mut last_progress_chunk_leftpad: f32 = 0.0;
    let mut current_pos: f32 = 0.0;
    let mut accum_pos: f32 = 0.0;

    // buttons for main player UI
    let FORWARD_BACKWARD_BTN_PAD = scale_calc(30, scale);
    let PREV_NEXT_BTN_LR_PAD = scale_calc(10, scale);
    let PAD_FROM_COVER = scale_calc(35, scale);
    let PAD_FROM_COVER_ABS = PAD_FROM_COVER + width;
    let mut prev = BoundingBoxTextInteractive::new(PREV_NEXT_BTN_LR_PAD, PREV_NEXT_BTN_LR_PAD + scale_calc(100, scale), PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + scale_calc(40, scale), 0, 0, String::from("Previous"), scale_calc(12, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());
    
    let back5sleft = (width/2)-9-scale_calc(52, scale)-FORWARD_BACKWARD_BTN_PAD;
    let mut back5s = BoundingBoxTextInteractive::new(back5sleft, back5sleft + scale_calc(40, scale), PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + scale_calc(40, scale), 0, 0, String::from("< 5s"), scale_calc(12, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());

    let playleft = (width/2)-scale_calc(9, scale);
    let mut play = BoundingBoxTextInteractive::new(playleft, playleft + scale_calc(40, scale), PAD_FROM_COVER_ABS-scale_calc(6, scale), PAD_FROM_COVER_ABS + scale_calc(40, scale), 0, 0, String::from("▶"), scale_calc(16, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());
    let mut pause = BoundingBoxTextInteractive::new(playleft, playleft + scale_calc(40, scale), PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + scale_calc(40, scale), 0, 0, String::from("| |"), scale_calc(12, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());
    
    let forward5sleft = (width/2)-9+20+FORWARD_BACKWARD_BTN_PAD;
    let mut forward5s = BoundingBoxTextInteractive::new(forward5sleft, forward5sleft + scale_calc(40, scale), PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + scale_calc(40, scale), 0, 0, String::from("5s >"), scale_calc(12, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());
    
    if disable_scrub {
        back5s.disable();
        forward5s.disable();
    }

    let nextleft = width - scale_calc(60, scale);
    let mut next = BoundingBoxTextInteractive::new(nextleft, nextleft + scale_calc(100, scale), PAD_FROM_COVER_ABS, PAD_FROM_COVER_ABS + scale_calc(40, scale), 0, 0, String::from("Next"), scale_calc(12, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());
    
    let closeleft = width-10-14-scale_calc(40, scale);
    let closetop = height-10-14-scale_calc(50, scale);
    let mut close = BoundingBoxTextInteractive::new(closeleft, width, closetop, height, 0, 0, String::from("✕"), scale_calc(20, scale), String::from("BLACK"), String::from("WHITE"), Elapsed::new());

    let mut volume_control = BoundingBoxTextInteractive::new(0, width, 0, height/2, 0, 0, String::new(), 1, String::from("BLACK"), String::from("WHITE"), Elapsed::new());

    // buttons for selector UI
    let mut height_8_segments_height = height/8;
    //let mut box_y_start = height_8_segments_height/2 + height_8_segments_height*2;
    let mut box_y_start = height - height_8_segments_height - height_8_segments_height*3/5 - height_8_segments_height;
    let mut box_y_end = box_y_start + height_8_segments_height;
    let mut numerals_y_start = box_y_start + height_8_segments_height*3/5 + scale_calc(8, scale);
    let mut letter_width = width/14;
    let mut lr_pad = letter_width;
    let mut selector_pad = 20;
    let mut numeral_display_pad = letter_width;

    let mut numerals: Vec<BoundingBoxTextInteractive> = Vec::new();
    for i in 1..11 {
        let value = i%10;
        numerals.push(BoundingBoxTextInteractive::new(letter_width*i, letter_width*(i+1), numerals_y_start, box_y_end, selector_pad, 0, value.to_string(), 20, String::from("WHITE"), String::from("BLACK"), Elapsed::new()));
    }
    numerals.push(BoundingBoxTextInteractive::new(letter_width*11, letter_width*12, numerals_y_start, box_y_end, selector_pad, 0, String::from("←"), 20, String::from("WHITE"), String::from("BLACK"), Elapsed::new()));
    numerals.push(BoundingBoxTextInteractive::new(letter_width*12, width, numerals_y_start, box_y_end, selector_pad, 0, String::from("OK"), 20, String::from("WHITE"), String::from("BLACK"), Elapsed::new()));

    let mut set_numeral_display = |v: &str, numerals: &Vec<BoundingBoxTextInteractive>, first_run: bool| {
        clear_canvas_partly("WHITE", box_y_start, lr_pad, width-lr_pad, numerals_y_start-5-box_y_start);
        draw_text(v, 34, box_y_start+height_8_segments_height/4 + scale_calc(2, scale), numeral_display_pad+selector_pad, "bold", "WHITE", "BLACK");
        if first_run {
            //draw_text("_____________________________________________________________", 20, box_y_start-20, numeral_display_pad+selector_pad, "regular", "WHITE", "BLACK");
            draw_text("_____________________________________________________________", 20, box_y_end-40, numeral_display_pad+selector_pad, "regular", "WHITE", "BLACK");
            for b in numerals {
                b.draw();
            }
        }
    };

    let mut current_track: Option<Track> = None;
    let mut current_album_is_new: bool = true;
    let mut current_album: String = String::new();
    let mut currently_paused: bool = true;
    let mut current_selection_panel_value: String = String::new();

    'eventloop: loop {
        // process new pointer events
        events_keeper.check_input();
        let mut e = events_keeper.rx.recv_timeout(Duration::from_millis(50));
        while let Ok(pointer_evt) = e {
            // only process these events if we have reign over the ui
            match pointer_evt {
                CapturedPointerEvent::PointerOn(coords) => {
                    if selector_visible {
                        let num_numerals = numerals.len();
                        for (b, i) in numerals.iter_mut().zip(0..num_numerals) {
                            if b.colliding_coords(&coords) {
                                if i < 10 {
                                    let value = (i+1)%10;
                                    current_selection_panel_value.push_str(&value.to_string());
                                    set_numeral_display(&current_selection_panel_value, &numerals, false);
                                } else if i == 10 {
                                    current_selection_panel_value = rem_last(&current_selection_panel_value).to_string();
                                    set_numeral_display(&current_selection_panel_value, &numerals, false);
                                } else {
                                    selector_visible = false;
                                    println!("ABC611");
                                    events_keeper.end_thread();
                                    println!("ABC612");
                                    events_keeper.ungrab();
                                    println!("ABC614");
                                    events_keeper.start_thread();
                                    println!("ABC615");
                                    if let Ok(new_track) = current_selection_panel_value.parse::<u32>() {
                                        sender.send(ControlMsg::SETTRACK(new_track-1));
                                    }
                                }
                                break;
                            }
                        }
                    } else if player_visible {
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
                            sender.send(ControlMsg::UIHIDDEN());
                            player_visible = false;
                            println!("ABC211");
                            events_keeper.end_thread();
                            println!("ABC212");
                            events_keeper.ungrab();
                            println!("ABC214");
                            events_keeper.start_thread();
                            println!("ABC215");
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
                    }
                },
                CapturedPointerEvent::PointerOff(coords) => {
                }
            }
            e = events_keeper.rx.recv_timeout(Duration::from_millis(50));
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
                    if player_visible {
                        draw_all(current_track.as_ref().unwrap(), vec![&mut prev,
                                                &mut back5s,
                                                &mut pause,
                                                &mut forward5s,
                                                &mut next,
                                                &mut close], &mut current_album_is_new, width, height, scale);
                    }
                },
                ControlMsg::LENGTH(length) => {
                    current_track_length = length;
                },
                ControlMsg::POS(pos) => {
                    println!("{} {} {}", pos, current_track_length, accum_pos);
                    if current_track_length != -1.0 {
                        let width_per_progress: u32 = 12;
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
                            if player_visible { clear_canvas_partly(color, width, start as u32, (end - start) as u32, 10); }
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
                    if player_visible {
                        draw_two_state(&b, &play, &pause);
                    }
                },
                ControlMsg::VOL(new_vol) => {
                    if player_visible {
                        draw_text_with_bg(&format!(" Volume {: >3} ", new_vol.to_string()), 9, width-50, 2, &root("assets/LinLibertine_M.otf").display().to_string(), "BLACK", "WHITE");
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
                    if current_track.is_some() {
                        selector_visible = true;
                        println!("ABC311");
                        events_keeper.end_thread();
                        println!("ABC312");
                        events_keeper.grab();
                        println!("ABC314");
                        events_keeper.start_thread();
                        println!("ABC315");
                        clear_canvas_partly("WHITE", box_y_start, lr_pad, width-lr_pad, height_8_segments_height);
                        set_numeral_display("", &numerals, true);
                    }
                } else if cmd.starts_with("ui") {
                    if let Some(current_track) = &current_track {
                        sender.send(ControlMsg::UIOPENED());
                        player_visible = true;
                        println!("ABC111");
                        events_keeper.end_thread();
                        println!("ABC112");
                        events_keeper.grab();
                        println!("ABC114");
                        events_keeper.start_thread();
                        println!("ABC115");
                        draw_all(current_track, vec![&mut prev,
                                                &mut back5s,
                                                &mut pause,
                                                &mut forward5s,
                                                &mut next,
                                                &mut close], &mut true, width, height, scale);
                        draw_two_state(&currently_paused, &play, &pause);
                        clear_canvas_partly("GRAYD", width, 0, last_progress_chunk_leftpad as u32, 10);
                    }
                }
            },
            Err(e) => {},
        }
    }
}
