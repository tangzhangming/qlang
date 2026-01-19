use std::sync::Arc;
use crossbeam_channel::{bounded, Sender, Receiver};
use parking_lot::Mutex;

type Task = Box<dyn FnOnce() + Send + 'static>;

pub struct IoThreadPool {
    workers: Vec<Worker>,
    sender: Sender<Task>,
}

impl IoThreadPool {
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = bounded(1024);
        let receiver = Arc::new(Mutex::new(receiver));

        let mut workers = Vec::with_capacity(size);
        for id in 0..size {
            workers.push(Worker::new(id, Arc::clone(&receiver)));
        }

        Self { workers, sender }
    }

    pub fn execute<F>(&self, f: F)
    where F: FnOnce() + Send + 'static {
        self.sender.send(Box::new(f)).unwrap();
    }
}

struct Worker {
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    fn new(id: usize, receiver: Arc<Mutex<Receiver<Task>>>) -> Self {
        let thread = std::thread::Builder::new()
            .name(format!("io-worker-{}", id))
            .spawn(move || {
                loop {
                    let task = {
                        let receiver = receiver.lock();
                        receiver.recv()
                    };
                    match task {
                        Ok(task) => task(),
                        Err(_) => break,
                    }
                }
            })
            .unwrap();
        Self { thread: Some(thread) }
    }
}
