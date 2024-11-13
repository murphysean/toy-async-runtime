use std::{
    fmt::{self, Debug},
    future::{Future, IntoFuture},
    io::stdin,
    pin::Pin,
    rc::Rc,
    sync::{mpsc::channel, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

use futures::FutureExt;

static mut TIME_CALLBACKS: Vec<(Instant, Waker)> = Vec::new();

fn main() {
    println!("Hello, world!");

    let (s, r) = channel();
    let mut executer = MyExecuter::new(r);
    println!("Executer: {:?}", executer);

    executer.spawn(s.clone(), here_we_go());
    println!("Executer: {:?}", executer);

    let e = Example { number: 2 };
    let e = Rc::new(e);
    let f = e.do_it();
    executer.spawn(s.clone(), f);

    loop {
        executer.run(s.clone());
        println!("Executer: {:?}", executer);

        //Go into a wait or park the thread, nothing to do
        let mut input_string = String::new();
        stdin()
            .read_line(&mut input_string)
            .expect("Failed to read");
        let trimmed = input_string.trim();
        if let Ok(i) = trimmed.parse::<usize>() {
            let response = s.send(i);
            if response.is_err() {
                panic!("Unable to Send");
            }
        }

        //Process the timer futures, see if any of them need to have their wakers called
        unsafe {
            let now = Instant::now();
            //TIME_CALLBACKS.iter_mut().filter_map(|i|now.checked_duration_since(i.0)).fo
            TIME_CALLBACKS.retain(|(instant, waker)| {
                if now.checked_duration_since(*instant).is_some() {
                    println!("Waking from callbacks... ");
                    waker.wake_by_ref();
                    return false;
                }
                true
            });
        }
    }
}

async fn here_we_go() {
    employee(1).await
}

async fn employee(id: u32) {
    println!("Employee {id} Asleep");
    //Some kind of Sleep for
    MyFuture::sleep(Duration::from_secs(10)).into_future().await;
    println!("Employee {id} Waking up");
    MyFuture::sleep(Duration::from_secs(1)).into_future().await;
    println!("Employee {id} Going to work");
    MyFuture::sleep(Duration::from_secs(1)).into_future().await;
    println!("Employee {id} Working");
    MyFuture::sleep(Duration::from_secs(8)).into_future().await;
    println!("Employee {id} Done");
}

trait Doiter {
    async fn do_it(self: Rc<Self>);
}

struct Example {
    number: i32,
}

impl Doiter for Example {
    async fn do_it(self: Rc<Self>) {
        MyFuture::sleep(Duration::from_secs(1)).await;
        println!("I'm doing it {}!", self.number);
    }
}

struct MyFuture {
    pub completed: Option<Instant>,
}

impl MyFuture {
    fn sleep(duration: Duration) -> Self {
        MyFuture {
            completed: Instant::now().checked_add(duration),
        }
    }
}

impl Future for MyFuture {
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if let Some(instant) = self.as_ref().completed {
            let now = Instant::now();
            if now.checked_duration_since(instant).is_some() {
                return Poll::Ready(());
            }
            unsafe {
                println!("Pushing Waker... Testing");
                if !TIME_CALLBACKS.iter().any(|i| i.1.will_wake(cx.waker())) {
                    println!("Pushing Waker");
                    TIME_CALLBACKS.push((instant, cx.waker().clone()));
                }
            }
        }
        Poll::Pending
    }
}

struct MyTask {
    future: Pin<Box<dyn Future<Output = ()>>>,
    times_polled: u32,
    last_poll_result: Poll<()>,
}

impl MyTask {
    fn from(future: impl Future<Output = ()> + 'static) -> Self {
        MyTask {
            future: Box::pin(future),
            times_polled: 0,
            last_poll_result: Poll::Pending,
        }
    }
}

impl Debug for MyTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("")
            .field(&self.times_polled)
            .field(&self.last_poll_result)
            .finish()
    }
}

#[derive(Debug)]
struct MyExecuter {
    tasks: Vec<MyTask>,
    reciever: std::sync::mpsc::Receiver<usize>,
}

impl MyExecuter {
    fn new(reciever: std::sync::mpsc::Receiver<usize>) -> Self {
        MyExecuter {
            tasks: Vec::new(),
            reciever,
        }
    }

    fn run(&mut self, sender: std::sync::mpsc::Sender<usize>) {
        self.tasks.retain(|i| i.last_poll_result.is_pending());
        self.reciever.try_iter().for_each(|i| {
            let Some(task) = self.tasks.get_mut(i) else {
                return;
            };
            let f = &mut task.future;
            let waker = Arc::new(MyWaker::new(i, sender.clone()));
            let waker = waker.into();
            let mut cx = Context::from_waker(&waker);
            match f.poll_unpin(&mut cx) {
                Poll::Pending => {
                    task.times_polled += 1;
                    task.last_poll_result = Poll::Pending;
                }
                Poll::Ready(()) => {
                    task.times_polled += 1;
                    task.last_poll_result = Poll::Ready(());
                    println!("{:?}", task);
                    println!("Task {i} done... Removing");
                    self.tasks.remove(i);
                }
            }
        });
    }

    fn spawn(
        &mut self,
        sender: std::sync::mpsc::Sender<usize>,
        future: impl Future<Output = ()> + 'static,
    ) {
        let task = MyTask::from(future);
        let response = sender.send(self.tasks.len());
        if response.is_err() {
            panic!("Unable to send on channel");
        }
        self.tasks.push(task);
    }
}

struct MyWaker {
    id: usize,
    sender: std::sync::mpsc::Sender<usize>,
}

impl MyWaker {
    fn new(id: usize, sender: std::sync::mpsc::Sender<usize>) -> Self {
        MyWaker { id, sender }
    }
}

impl Wake for MyWaker {
    fn wake(self: Arc<Self>) {
        println!("MyWaker: {} wake", self.id);
        let response = self.sender.send(self.id);
        if response.is_err() {
            panic!("Unable to send on channel");
        }
    }
}
