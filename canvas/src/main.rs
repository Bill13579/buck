use std::env;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!("wrong number of arguments");
        return;
    }
    let mut width = 0;
    let mut height = 0;
    if let Ok(w) = args[1].parse::<usize>() {
        width = w;
    } else {
        println!("width could not be parsed");
        return;
    }
    if let Ok(h) = args[2].parse::<usize>() {
        height = h;
    } else {
        println!("height could not be parsed");
        return;
    }
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
                    .with_title("L:A_N:application_ID:buckuicanvas")
                    .with_resizable(false).with_maximized(true)
                    .build(&event_loop)
                    .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => *control_flow = ControlFlow::Exit,
            _ => (),
        }
    });
}