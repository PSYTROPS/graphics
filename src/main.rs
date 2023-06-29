use graphics::Renderer;
use graphics::scene::Scene;
use graphics::scene_set::SceneSet;
use graphics::environment::Environment;

struct Inputs {
    //Translation
    forward: bool,
    backward: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
    //Rotation
    pitch_up: bool,
    pitch_down: bool,
    yaw_left: bool,
    yaw_right: bool
}

fn main() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let window = video.window("Graphics", 1024, 1024).vulkan().resizable().build().unwrap();
    let mut renderer = Renderer::new(&window).expect("Renderer creation error");
    renderer.camera.pos[2] = 1.0;
    //Load scene
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.push("assets");
    //path.push("Cube.gltf");
    path.push("MetalRoughSpheres.glb");
    //path.push("DamagedHelmet.glb");
    //path.push("SciFiHelmet.gltf");
    //path.push("bottle.glb");
    let scene = Scene::load_gltf(path).unwrap();
    let environment = Environment::new(
        renderer.base.clone(),
        &mut renderer.transaction.borrow_mut(),
        include_bytes!("../assets/specular.ktx2"),
        include_bytes!("../assets/diffuse.ktx2"),
        include_bytes!("../assets/specular.ktx2")
    ).unwrap();
    let mut scene_set = SceneSet::new(&renderer, environment).unwrap();
    scene_set.push_scene(&scene, &renderer);
    /*
    scene_set.lights[0] = PointLight {
        pos: [1.0, 0.0, 1.0, 0.0],
        color: [1.0, 1.0, 1.0, 1.0],
        intensity: 0.0,
        range: 8.0
    };
    */
    //Event loop
    let mut event_pump = sdl.event_pump().unwrap();
    let mut now = std::time::Instant::now();
    let mut inputs = Inputs {
        //Translation
        forward: false,
        backward: false,
        left: false,
        right: false,
        up: false,
        down: false,
        //Rotation
        pitch_up: false,
        pitch_down: false,
        yaw_left: false,
        yaw_right: false 
    };
    'main: loop {
        let delta = now.elapsed();
        now = std::time::Instant::now();
        for event in event_pump.poll_iter() {
            match event {
                sdl2::event::Event::Quit{..} => break 'main,
                sdl2::event::Event::KeyDown{
                    keycode: Some(keycode), ..
                } => match keycode {
                    //Translation
                    sdl2::keyboard::Keycode::W => inputs.forward = true,
                    sdl2::keyboard::Keycode::S => inputs.backward = true,
                    sdl2::keyboard::Keycode::A => inputs.left = true,
                    sdl2::keyboard::Keycode::D => inputs.right = true,
                    sdl2::keyboard::Keycode::Z => inputs.up = true,
                    sdl2::keyboard::Keycode::X => inputs.down = true,
                    //Rotation
                    sdl2::keyboard::Keycode::Up => inputs.pitch_up = true,
                    sdl2::keyboard::Keycode::Down => inputs.pitch_down = true,
                    sdl2::keyboard::Keycode::Left => inputs.yaw_left = true,
                    sdl2::keyboard::Keycode::Right => inputs.yaw_right = true,
                    //Quit
                    sdl2::keyboard::Keycode::Q => break 'main,
                    _ => ()
                },
                sdl2::event::Event::KeyUp{
                    keycode: Some(keycode), ..
                } => match keycode {
                    //Translation
                    sdl2::keyboard::Keycode::W => inputs.forward = false,
                    sdl2::keyboard::Keycode::S => inputs.backward = false,
                    sdl2::keyboard::Keycode::A => inputs.left = false,
                    sdl2::keyboard::Keycode::D => inputs.right = false,
                    sdl2::keyboard::Keycode::Z => inputs.up = false,
                    sdl2::keyboard::Keycode::X => inputs.down = false,
                    //Rotation
                    sdl2::keyboard::Keycode::Up => inputs.pitch_up = false,
                    sdl2::keyboard::Keycode::Down => inputs.pitch_down = false,
                    sdl2::keyboard::Keycode::Left => inputs.yaw_left = false,
                    sdl2::keyboard::Keycode::Right => inputs.yaw_right = false,
                    _ => ()
                },
                _ => ()
            }
        }
        //Movement
        //Translation
        let mut direction = [0.0; 3];
        let speed = 1.0;
        if inputs.forward {direction[0] += speed * delta.as_secs_f32();}
        if inputs.backward {direction[0] -= speed * delta.as_secs_f32();}
        if inputs.left {direction[1] -= speed * delta.as_secs_f32();}
        if inputs.right {direction[1] += speed * delta.as_secs_f32();}
        if inputs.up {direction[2] += speed * delta.as_secs_f32();}
        if inputs.down {direction[2] -= speed * delta.as_secs_f32();}
        renderer.camera.locomote(direction[0], direction[1], direction[2]);
        //Rotation
        let mut rotation = [0.0; 2];
        if inputs.pitch_up {rotation[0] += 1.0 * delta.as_secs_f32();}
        if inputs.pitch_down {rotation[0] -= 1.0 * delta.as_secs_f32();}
        if inputs.yaw_left {rotation[1] += 1.0 * delta.as_secs_f32();}
        if inputs.yaw_right {rotation[1] -= 1.0 * delta.as_secs_f32();}
        renderer.camera.rotate(rotation[0], rotation[1]);
        //Draw
        renderer.draw(&scene_set).unwrap();
    }
}
