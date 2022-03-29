// toc.rs
// Table of Contents generator

use std::{fs::OpenOptions, io::Write, path::PathBuf};

use genpdf::{self, Alignment, Mm, Element, style::{Style, Color}, elements::{Paragraph, TableLayout}, Margins};
use textwrap::wrap;

use crate::{Track, log, result, error, process_runner::{quick_write, quick_run}};
use crate::read_config::root;

pub fn gentoc(tracks: &Vec<Track>, pdf_output_path: PathBuf) {

    log!("gentoc", "starting...");

    let font_family = result!(genpdf::fonts::from_files(root("assets"), "Bookerly", None));

    let mut doc = genpdf::Document::new(font_family);

    doc.set_title("Buck - Table of Contents");

    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(Margins::trbl(20 as i8, 10 as i8, 10 as i8, 10 as i8));
    doc.set_page_decorator(decorator);

    doc.set_minimal_conformance();
    doc.set_line_spacing(2.0);

    let mut header = genpdf::elements::Paragraph::default();
    header.push_styled("Table of Contents", Style::new().with_font_size(40));
    header.set_alignment(Alignment::Left);
    doc.push(header.padded(Margins::trbl(0 as i8, 0 as i8, 10 as i8, 0 as i8)));

    let calculate_optimal_layout = |text1len: usize, text2len: usize, text1min: usize, text2min: usize| -> Vec<usize> {
        let mut left = ((text1len as f64 / (text1len + text2len) as f64) * 100.0).floor() as usize;
        let mut right = 100 - left;
        if right < text2min {
            right = text2min;
            left = 100 - text2min;
        }
        if left < text1min {
            left = text1min;
            right = 100 - text1min;
        }
        vec![left, right]
    };

    let wrap_and_turn_into_a_paragraph = |s: &str, max_chars: u32, style: Style| -> Paragraph {
        let lines = wrap(s, max_chars as usize);
        let mut p = genpdf::elements::Paragraph::default();
        for l in lines {
            p.push_styled(l, style.clone());
        }
        p
    };

    let gen_album_layout = |title: &str, artist: &str| -> TableLayout {
        let mut title_p = genpdf::elements::Paragraph::default().styled_string(title, Style::new().with_font_size(24).italic());
        title_p.set_alignment(Alignment::Left);
        let mut artist_p = genpdf::elements::Paragraph::default().styled_string(artist, Style::new().with_font_size(22).with_color(Color::Rgb(59, 59, 59)));
        artist_p.set_alignment(Alignment::Right);
        let mut table = genpdf::elements::TableLayout::new(calculate_optimal_layout(title.len(), artist.len(), 40, 20));
        table.row()
            .element(title_p)
            .element(artist_p)
            .push();
        table
    };

    let gen_song_layout = |title: &str, artist: &str, pos: u32, first_track: &mut bool| -> TableLayout {
        let mut pos_style = Style::new().with_font_size(20);
        if *first_track {
            pos_style = Style::new().with_font_size(21).italic();
            *first_track = false;
        }
        let mut pos_p = genpdf::elements::Paragraph::default().styled_string(format!("{}. ", pos.to_string()), pos_style);
        pos_p.set_alignment(Alignment::Left);
        let mut title_p = genpdf::elements::Paragraph::default().styled_string(title, Style::new().with_font_size(22));
        title_p.set_alignment(Alignment::Left);
        let mut artist_p = genpdf::elements::Paragraph::default().styled_string(artist, Style::new().with_font_size(20).with_color(Color::Rgb(117, 117, 117)));
        artist_p.set_alignment(Alignment::Right);
        let mut l = calculate_optimal_layout(title.len(), artist.len(), 40, 25);
        l.insert(0, 11);
        let mut table = genpdf::elements::TableLayout::new(l);
        table.row()
            .element(pos_p)
            .element(title_p)
            .element(artist_p)
            .push();
        table
    };

    let mut current_album = &tracks[0];
    let mut first_track = true;
    doc.push(gen_album_layout(&current_album.album, &current_album.artist).padded(Margins::trbl(15 as i8, 0 as i8, 3 as i8, 0 as i8)));
    for (t, i) in tracks.iter().zip(0..tracks.len()) {
        if current_album.album != t.album {
            first_track = true;
            current_album = &t;
            doc.push(gen_album_layout(&current_album.album, &current_album.artist).padded(Margins::trbl(15 as i8, 0 as i8, 2 as i8, 0 as i8)));
        }
        doc.push(gen_song_layout(&t.title, &t.artist, i as u32+1, &mut first_track).padded(Margins::trbl(9 as i8, 0 as i8, 0 as i8, 0 as i8)));
    }

    log!("gentoc", "starting render of T.O.C., this might take a while...");
    println!("starting render of T.O.C., this might take a while...");
    log!("gentoc", "ready, starting now");

    quick_write(2, "* Rendering the Table of Contents...");
    quick_write(3, "   (this might take a while)");
    println!("{}", pdf_output_path.display().to_string());
    doc.render_to_file(pdf_output_path).expect("failed to write T.O.C. to filesystem");
    quick_write(4, "* Done!");

    log!("gentoc", "write complete");
    println!("[*] write complete");
}