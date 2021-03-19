#![feature(get_mut_unchecked)]
use memory_rs::internal::{
    injections::{Detour, Inject, Injection},
    memory::resolve_module_path,
};
use std::ffi::CString;
use winapi::um::consoleapi::AllocConsole;
use winapi::um::libloaderapi::{FreeLibraryAndExitThread, GetModuleHandleA};
use winapi::um::wincon::FreeConsole;
use winapi::um::winuser;
use winapi::{shared::minwindef::LPVOID, um::libloaderapi::GetProcAddress};

use log::*;
use simplelog::*;

mod camera;
mod globals;
mod input;
mod utils;

use camera::*;
use globals::*;
use input::*;
use utils::{check_key_press, error_message, handle_keyboard, Input};

use std::io::{self, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

fn write_red(msg: &str) -> io::Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
    writeln!(&mut stdout, "{}", msg)?;
    stdout.reset()
}

unsafe extern "system" fn wrapper(lib: LPVOID) -> u32 {
    AllocConsole();
    {
        let mut path = resolve_module_path(lib).unwrap();
        path.push("botw.log");
        CombinedLogger::init(vec![
            TermLogger::new(
                log::LevelFilter::Info,
                Config::default(),
                TerminalMode::Mixed,
            ),
            WriteLogger::new(
                log::LevelFilter::Info,
                Config::default(),
                std::fs::File::create(path).unwrap(),
            ),
        ])
        .unwrap();

        match patch(lib) {
            Ok(_) => (),
            Err(e) => {
                let msg = format!("Something went wrong:\n{}", e);
                error!("{}", msg);
                error_message(&msg);
            }
        }
    }

    FreeConsole();
    FreeLibraryAndExitThread(lib as _, 0);
    0
}

#[derive(Debug)]
struct CameraOffsets {
    camera: usize,
    rotation_vec1: usize,
    rotation_vec2: usize,
}

fn get_camera_function() -> Result<CameraOffsets, Box<dyn std::error::Error>> {
    let function_name = CString::new("PPCRecompiler_getJumpTableBase").unwrap();
    let proc_handle = unsafe { GetModuleHandleA(std::ptr::null_mut()) };
    let func = unsafe { GetProcAddress(proc_handle, function_name.as_ptr()) };

    if (func as usize) == 0x0 {
        return Err("Func returned was empty".into());
    }
    let func: extern "C" fn() -> usize = unsafe { std::mem::transmute(func) };

    let addr = (func)();

    if addr == 0x0 {
        return Err(
            "Jump table was empty, Check you're running the game and using recompiler profile"
                .into(),
        );
    }

    let array = unsafe { std::slice::from_raw_parts(addr as *const usize, 0x8800000 / 0x8) };
    let original_bytes = [
        0x45_u8, 0x0F, 0x38, 0xF1, 0xB4, 0x05, 0xC4, 0x05, 0x00, 0x00,
    ];

    // As Exzap said, "It will only compile it once its executed. Before that the table points to a placeholder function"
    // So we'll wait until the game is in the world and the code will be recompiled, then the pointer should be changed to the right function.
    // Once is resolved, we can lookup the rest of the functions since the camera we assume the camera is active
    let dummy_pointer = array[0];
    info!("Waiting for the game to start");
    let camera_offset = loop {
        let function_start = array[0x2C05484 / 4];

        if dummy_pointer != function_start {
            info!("Pointer found");
            break function_start + 0x6C;
        }
        std::thread::sleep(std::time::Duration::from_secs(1))
    };

    let camera_bytes = unsafe { std::slice::from_raw_parts((camera_offset) as *const u8, 10) };
    if camera_bytes != original_bytes {
        return Err(format!(
            "Function signature doesn't match, This can mean two things:\n\n\
            * You're using a pre 2016 CPU (your cpu doesn't support `movbe`)\n\
            * You're not using the version described on the README.md\n\
            {:x?} != {:x?}",
            camera_bytes, original_bytes
        )
        .into());
    }

    let rotation_vec1 = array[0x2C085FC / 4] + 0x157;
    let rotation_vec2 = array[0x2e57fdc / 4] + 0x7f;

    Ok(CameraOffsets {
        camera: camera_offset,
        rotation_vec1,
        rotation_vec2,
    })
}

fn patch(_lib: LPVOID) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Breath of the Wild VR Driver by @etra0, v{}",
        utils::get_version()
    );
    write_red("If you close this window the game will close. Use HOME to deattach the freecamera (will close this window as well).")?;
    println!("{}", utils::INSTRUCTIONS);
    write_red("Controller input will only be detected if Xinput is used in the Control settings, otherwise use the keyboard.")?;

    let mut input = Input::new();

    let mut active = false;

    let camera_struct = get_camera_function()?;
    info!("{:x?}", camera_struct);
    let camera_pointer = camera_struct.camera;
    info!("Camera function camera_pointer: {:x}", camera_pointer);

    let mut cam = unsafe {
        Detour::new(
            camera_pointer,
            14,
            &asm_get_camera_data as *const u8 as usize,
            Some(&mut g_get_camera_data),
        )
    };

    let mut nops = vec![
        // Camera pos
        // Injection::new(camera_struct.camera + 0x1C8, vec![0x90; 10]),
        // Injection::new(camera_struct.camera + 0x4C, vec![0x90; 10]),

        // Focus
        Injection::new(camera_struct.camera + 0x17, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x98, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x1DF, vec![0x90; 10]),
        // Fov
        Injection::new(camera_struct.camera + 0xAF, vec![0x90; 10]),
        // Rotation
        Injection::new(camera_struct.rotation_vec1, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec1 + 0x3E, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec1 + 0x9B, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec2, vec![0x90; 7]),
        Injection::new(camera_struct.rotation_vec2 - 0x14, vec![0x90; 7]),
        Injection::new(camera_struct.rotation_vec2 - 0x28, vec![0x90; 7]),
    ];

    cam.inject();
    let mut input_poll = InputPoll::new("127.0.0.1:25565")?;
    input_poll.start_polling();

    loop {
        handle_keyboard(&mut input);
        if input.deattach || check_key_press(winuser::VK_HOME) {
            input_poll.stop_polling()?;
            info!("Exiting");
            break;
        }

        input_poll.get_input(&mut input);
        dbg!(&input);

        input.is_active = active;
        if input.change_active {
            active = !active;

            unsafe {
                g_camera_active = active as u8;
            }
            info!("Camera is {}", active);

            if active {
                nops.inject();
            } else {
                nops.remove_injection();
            }

            input.change_active = false;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        unsafe {
            // If we don't have the camera struct we need to skip it right away
            if g_camera_struct == 0x0 {
                continue;
            }

            let gc = g_camera_struct as *mut GameCamera;

            // println!("{:?}", *gc);
            if !active {
                input.fov = (*gc).fov.to_fbe();
                continue;
            }

            (*gc).consume_input(&input);
        }

        input.reset();

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

memory_rs::main_dll!(wrapper);
