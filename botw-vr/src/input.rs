use crate::utils::Input;
use std::convert::TryInto;
use std::sync::{
    mpsc::{Receiver, Sender},
    Arc,
};
use std::thread::JoinHandle;
use std::{
    net::UdpSocket,
    sync::atomic::{AtomicBool, Ordering},
};

pub struct InputPoll {
    socket: Arc<UdpSocket>,
    sender: Sender<[f32; 0x18]>,
    receiver: Receiver<[f32; 0x18]>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl InputPoll {
    pub fn new(bound: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let socket = Arc::new(UdpSocket::bind(bound)?);
        socket.set_nonblocking(true)?;
        let (sender, receiver) = std::sync::mpsc::channel();

        Ok(Self {
            socket,
            sender,
            receiver,
            running: Arc::new(AtomicBool::new(false)),
            thread: None,
        })
    }

    pub fn start_polling(&mut self) {
        self.running.store(true, Ordering::SeqCst);
        let socket = self.socket.clone();
        let running = self.running.clone();
        let sender = self.sender.clone();

        self.thread = Some(std::thread::spawn(move || {
            let mut buff = [0_u8; 0x18 * 4];
            while running.load(Ordering::SeqCst) {
                match socket.recv_from(&mut buff) {
                    Ok(_) => (),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                    Err(e) => panic!("Some Error with the recv_from: {}", e),
                };

                // TODO: Revisit this once Melon checks the endianness issue.
                let result = buff
                    .chunks_exact(4)
                    .map(|x| f32::from_bits(u32::from_be_bytes(x.try_into().unwrap())))
                    .collect::<Vec<f32>>()
                    .try_into()
                    .unwrap();
                sender.send(result).unwrap();
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
    pub fn get_input(&self, input: &mut Input) -> Result<(), Box<dyn std::error::Error>> {
        match self.receiver.try_recv() {
            Ok(buffer) => {
                input.delta_focus = (buffer[4], buffer[3]);
                Ok(())
            }

            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(()),
            Err(e) => Err(e.into())
        }
    }

    pub fn stop_polling(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Stopping poll");
        self.running.store(false, Ordering::SeqCst);

        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        } else {
            return Err("No thread was started".into());
        }

        Ok(())
    }
}

impl Drop for InputPoll {
    fn drop(&mut self) {
        self.stop_polling().unwrap();
    }
}
