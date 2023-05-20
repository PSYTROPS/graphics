mod renderer;

fn main() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video.window("Graphics", 512, 512).vulkan().resizable().build().unwrap();
    //let surface = window.vulkan_create_surface(instance).unwrap();
    let mut canvas = window.into_canvas().build().unwrap();
    canvas.set_draw_color(sdl2::pixels::Color::RGB(255, 0, 0));
    canvas.clear();
    canvas.present();
    //Event loop
    let mut event_pump = sdl.event_pump().unwrap();
    'main: loop {
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit{..} => break 'main,
                sdl2::event::Event::KeyDown{
                    keycode: Some(keycode), ..
                } => match keycode {
                    sdl2::keyboard::Keycode::Q => break 'main,
                    _ => ()
                },
                _ => ()
            }
        }
    }
}
