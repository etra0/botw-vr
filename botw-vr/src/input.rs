use std::sync::Arc;
use std::thread::JoinHandle;
use std::{
    net::UdpSocket,
    sync::atomic::{AtomicBool, Ordering},
};
use crate::utils::Input;

pub struct InputPoll {
    socket: Arc<UdpSocket>,
    buffer: Arc<[f32; 0x18]>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl InputPoll {
    pub fn new(bound: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = Arc::new(UdpSocket::bind(bound)?);
        socket.set_nonblocking(true)?;

        Ok(Self {
            socket,
            buffer: Arc::new([0.; 0x18]),
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        })
    }

    pub fn start_polling(&mut self) {
        self.running.store(true, Ordering::SeqCst);
        let mut buff = self.buffer.clone();
        let socket = self.socket.clone();
        let running = self.running.clone();

        self.thread = Some(std::thread::spawn(move || {
            let mut_buff = unsafe { Arc::get_mut_unchecked(&mut buff) };
            let mut slice_as_u8: &mut [u8] = unsafe {
                std::slice::from_raw_parts_mut(mut_buff.as_mut_ptr() as *mut u8, 4 * mut_buff.len())
            };
            let mut state = running.load(Ordering::SeqCst);
            println!("Should be running? {:?}", state);
            while state {
                match socket.recv_from(&mut slice_as_u8) {
                    Ok(_) => (),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => (),
                    Err(e) => panic!("Some Error with the recv_from")
                };
                state = running.load(Ordering::SeqCst);
            }
            println!("Stopping polling from thread");
        }));
    }

    /// 0x00 (float) (position x) 0
    /// 0x04 (float) (position y) 1
    /// 0x08 (float) (position z) 2
    /// 0x0C (float) (rotation x) 3
    /// 0x10 (float) (rotation y) 4
    /// 0x14 (float) (rotation z) 5
    pub fn get_input(&self, input: &mut Input) {
        let original_buffer = unsafe { std::slice::from_raw_parts(self.buffer.as_ptr() as *const u32, 0x18) };
        let buff = original_buffer
            .iter()
            .map(|x| f32::from_bits(x.to_be()))
            .collect::<Vec<f32>>();
        input.delta_focus = (buff[4], buff[3]);
    }

    pub fn stop_polling(self) -> Result<(), String> {
        println!("Stopping poll");
        self.running.store(false, Ordering::SeqCst);

        println!("Status of polling: {:?}", self.running.load(Ordering::SeqCst));

        if let Some(t) = self.thread {
            t.join().unwrap();
        } else {
            return Err("No thread was started".into());
        }

        Ok(())
    }
}