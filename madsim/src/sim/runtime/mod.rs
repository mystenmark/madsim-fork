//! The madsim runtime.

use super::*;
use crate::task::{JoinHandle, NodeId, InitFn};
use std::{
    any::TypeId,
    collections::HashMap,
    future::Future,
    net::IpAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::runtime::TryCurrentError;

pub(crate) mod context;

/// Builder module
pub mod builder;

/// The madsim runtime.
///
/// The runtime provides basic components for deterministic simulation,
/// including a [random number generator], [timer], [task scheduler], and
/// simulated [network] and [file system].
///
/// [random number generator]: crate::rand
/// [timer]: crate::time
/// [task scheduler]: crate::task
/// [network]: crate::net
/// [file system]: crate::fs
pub struct Runtime {
    rand: rand::GlobalRng,
    task: task::Executor,
    handle: Handle,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    /// Create a new runtime instance with default seed and config.
    pub fn new() -> Self {
        Self::with_seed_and_config(0, Config::default())
    }

    /// Create a new runtime instance with given seed and config.
    pub fn with_seed_and_config(seed: u64, config: Config) -> Self {
        #[cfg(feature = "logger")]
        init_logger();

        let rand = rand::GlobalRng::new_with_seed(seed);
        let task = task::Executor::new(rand.clone());
        let handle = Handle {
            rand: rand.clone(),
            time: task.time_handle().clone(),
            task: task.handle().clone(),
            sims: Default::default(),
            config,
        };
        let rt = Runtime { rand, task, handle };
        rt.add_simulator::<fs::FsSim>();
        rt.add_simulator::<net::NetSim>();
        rt
    }

    /// Register a simulator.
    pub fn add_simulator<S: plugin::Simulator>(&self) {
        let mut sims = self.handle.sims.lock().unwrap();
        let sim = Arc::new(S::new(
            &self.handle.rand,
            &self.handle.time,
            &self.handle.config,
        ));
        // create node for supervisor
        sim.create_node(NodeId::zero());
        sims.insert(TypeId::of::<S>(), sim);
    }

    /// enter
    pub fn enter(&self) -> EnterGuard<'_> {
        self.handle.enter()
    }

    /// Return a handle to the runtime.
    ///
    /// The returned handle can be used by the supervisor (future in [block_on])
    /// to control the whole system. For example, kill a node or disconnect the
    /// network.
    ///
    /// [block_on]: Runtime::block_on
    pub fn handle(&self) -> &Handle {
        &self.handle
    }

    /// Create a node.
    ///
    /// The returned handle can be used to spawn tasks that run on this node.
    pub fn create_node(&self) -> NodeBuilder<'_> {
        self.handle.create_node()
    }

    /// Spawn a task.
    ///
    /// Normally this will not be called, unless something is creating a runtime manually.
    /// If that happens, spawn the task into the current node instead.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        task::spawn(future)
    }

    /// Run a future to completion on the runtime. This is the runtime’s entry point.
    ///
    /// This runs the given future on the current thread until it is complete.
    ///
    /// # Example
    ///
    /// ```
    /// use madsim::runtime::Runtime;
    ///
    /// let rt = Runtime::new();
    /// let ret = rt.block_on(async { 1 });
    /// assert_eq!(ret, 1);
    /// ```
    ///
    /// Unlike usual async runtime, when there is no runnable task, it will
    /// panic instead of blocking.
    ///
    /// ```should_panic
    /// use madsim::runtime::Runtime;
    /// use futures::future::pending;
    ///
    /// Runtime::new().block_on(pending::<()>());
    /// ```
    pub fn block_on<F: Future>(&self, future: F) -> F::Output {
        let _guard = crate::context::enter(self.handle.clone());
        self.task.block_on(future)
    }

    /// Set a time limit of the execution.
    ///
    /// The runtime will panic when time limit exceeded.
    ///
    /// # Example
    ///
    /// ```should_panic
    /// use madsim::{runtime::Runtime, time::{sleep, Duration}};
    ///
    /// let mut rt = Runtime::new();
    /// rt.set_time_limit(Duration::from_secs(1));
    ///
    /// rt.block_on(async {
    ///     sleep(Duration::from_secs(2)).await;
    /// });
    /// ```
    pub fn set_time_limit(&mut self, limit: Duration) {
        self.task.set_time_limit(limit);
    }

    /// Enable determinism check during the simulation.
    ///
    /// # Example
    ///
    /// ```should_panic
    /// use madsim::{runtime::Runtime, time::{sleep, Duration}};
    /// use rand::Rng;
    ///
    /// let f = || async {
    ///     for _ in 0..10 {
    ///         madsim::rand::thread_rng().gen::<u64>();
    ///         // introduce non-determinism
    ///         let rand_num = rand::thread_rng().gen_range(0..10);
    ///         sleep(Duration::from_nanos(rand_num)).await;
    ///     }
    /// };
    ///
    /// let mut rt = Runtime::new();
    /// rt.enable_determinism_check(None);      // enable log
    /// rt.block_on(f());
    /// let log = rt.take_rand_log();           // take log for next turn
    ///
    /// let mut rt = Runtime::new();
    /// rt.enable_determinism_check(log);       // enable check
    /// rt.block_on(f());                       // run the same logic again,
    ///                                         // should panic here.
    /// ```
    pub fn enable_determinism_check(&self, log: Option<rand::Log>) {
        assert_eq!(
            self.task.time_handle().elapsed(),
            Duration::default(),
            "deterministic check must be set at init"
        );
        if let Some(log) = log {
            self.rand.enable_check(log);
        } else {
            self.rand.enable_log();
        }
    }

    /// Take random log so that you can check determinism in the next turn.
    pub fn take_rand_log(self) -> Option<rand::Log> {
        self.rand.take_log()
    }
}

type Callback = std::sync::Arc<dyn Fn() + Send + Sync>;

/// Supervisor handle to the runtime.
#[derive(Clone)]
pub struct Handle {
    pub(crate) rand: rand::GlobalRng,
    pub(crate) time: time::TimeHandle,
    pub(crate) task: task::TaskHandle,
    pub(crate) sims: Arc<Mutex<HashMap<TypeId, Arc<dyn plugin::Simulator>>>>,
    pub(crate) config: Config,
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(fmt, "Handle")
    }
}

/// EnterGuard
#[derive(Debug)]
#[must_use = "Creating and dropping a guard does nothing"]
pub struct EnterGuard<'a> {
    _guard: crate::context::EnterGuard,
    _handle_lifetime: std::marker::PhantomData<&'a Handle>,
}

// Tokio api
impl Handle {
    /// enter
    pub fn enter(&self) -> EnterGuard<'_> {
        EnterGuard {
            _guard: crate::context::enter(self.clone()),
            _handle_lifetime: std::marker::PhantomData,
        }
    }

    /// try to get the current handle
    pub fn try_current() -> Result<Self, TryCurrentError> {
        Ok(Self::current())
    }

    /// Spawn task on current node
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        task::spawn(future)
    }

    /*
    #[track_caller]

    #[track_caller]
    pub fn spawn_blocking<F, R>(&self, func: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        self.as_inner().spawn_blocking(self, func)
    }

    #[track_caller]
    pub(crate) fn spawn_named<F>(&self, future: F, _name: Option<&str>) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let id = crate::runtime::task::Id::next();
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let future = crate::util::trace::task(future, "task", _name, id.as_u64());
        self.spawner.spawn(future, id)
    }

    pub(crate) fn shutdown(mut self) {
        self.spawner.shutdown();
    }
    */
}

impl Handle {
    /// Returns a [`Handle`] view over the currently running [`Runtime`].
    ///
    /// ## Panic
    ///
    /// This will panic if called outside the context of a Madsim runtime.
    ///
    /// ```should_panic
    /// let handle = madsim::runtime::Handle::current();
    /// ```
    pub fn current() -> Self {
        context::current(|h| h.clone())
    }

    /// Kill a node.
    ///
    /// - All tasks spawned on this node will be killed immediately.
    /// - All data that has not been flushed to the disk will be lost.
    pub fn kill(&self, id: NodeId) {
        self.task.kill(id);
        for sim in self.sims.lock().unwrap().values() {
            sim.reset_node(id);
        }
    }

    /// Restart a node。
    pub fn restart(&self, id: NodeId) {
        self.task.restart(id);
        for sim in self.sims.lock().unwrap().values() {
            sim.reset_node(id);
        }
    }

    /// Pause the execution of a node.
    pub fn pause(&self, id: NodeId) {
        self.task.pause(id);
    }

    /// Resume the execution of a node.
    pub fn resume(&self, id: NodeId) {
        self.task.resume(id);
    }

    /// Create a node which will be bound to the specified address.
    pub fn create_node(&self) -> NodeBuilder<'_> {
        NodeBuilder::new(self)
    }

    /// Return a handle of the specified node.
    pub fn get_node(&self, id: NodeId) -> Option<NodeHandle> {
        self.task.get_node(id).map(|task| NodeHandle { task })
    }
}

/// Builds a node with custom configurations.
pub struct NodeBuilder<'a> {
    handle: &'a Handle,
    name: Option<String>,
    ip: Option<IpAddr>,
    init: Option<Arc<InitFn>>,
}

impl<'a> NodeBuilder<'a> {
    fn new(handle: &'a Handle) -> Self {
        NodeBuilder {
            handle,
            name: None,
            ip: None,
            init: None,
        }
    }

    /// Names the node.
    ///
    /// The default name is node ID.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the initial task for the node.
    ///
    /// This task will be automatically respawned after crash.
    pub fn init<F>(mut self, future: impl Fn() -> F + 'static + Sync + Send) -> Self
    where
        F: Future + 'static,
    {
        self.init = Some(Arc::new(move |handle| {
            handle.spawn_local(future());
        }));
        self
    }

    /// Set one IP address of the node.
    pub fn ip(mut self, ip: IpAddr) -> Self {
        self.ip = Some(ip);
        self
    }

    /// Build a node.
    pub fn build(self) -> NodeHandle {
        let task = self.handle.task.create_node(self.name, self.init);
        for sim in self.handle.sims.lock().unwrap().values() {
            sim.create_node(task.id());
            if let Some(ip) = self.ip {
                if let Some(net) = sim.downcast_ref::<net::NetSim>() {
                    net.set_ip(task.id(), ip)
                }
            }
        }
        NodeHandle { task }
    }
}

/// Handle to a node.
#[derive(Clone)]
pub struct NodeHandle {
    task: task::TaskNodeHandle,
}

impl NodeHandle {
    /// Returns the node ID.
    pub fn id(&self) -> NodeId {
        self.task.id()
    }

    /// Spawn a future onto the runtime.
    pub fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.task.spawn(future)
    }
}

#[cfg(feature = "logger")]
fn init_logger() {
    use env_logger::fmt::Color;
    use std::io::Write;
    use std::sync::Once;
    static LOGGER_INIT: Once = Once::new();
    LOGGER_INIT.call_once(|| {
        let start = std::env::var("MADSIM_LOG_TIME_START")
            .ok()
            .map(|s| Duration::from_secs_f64(s.parse::<f64>().unwrap()));
        let mut builder = env_logger::Builder::from_default_env();
        builder.format(move |buf, record| {
            let mut style = buf.style();
            style.set_color(Color::Black).set_intense(true);
            let mut level_style = buf.style();
            level_style.set_color(match record.level() {
                log::Level::Error => Color::Red,
                log::Level::Warn => Color::Yellow,
                log::Level::Info => Color::Green,
                log::Level::Debug => Color::Blue,
                log::Level::Trace => Color::Cyan,
            });
            if let Some(time) = crate::time::TimeHandle::try_current() {
                if matches!(start, Some(t0) if time.elapsed() < t0) {
                    return write!(buf, "");
                }
                let task = crate::context::current_task();
                writeln!(
                    buf,
                    "{}{:>5}{}{:.6}s{}{}{}{:>10}{} {}",
                    style.value('['),
                    level_style.value(record.level()),
                    style.value("]["),
                    time.elapsed().as_secs_f64(),
                    style.value("]["),
                    task.name,
                    style.value("]["),
                    record.target(),
                    style.value(']'),
                    record.args()
                )
            } else {
                writeln!(
                    buf,
                    "{}{:>5}{}{:>10}{} {}",
                    style.value('['),
                    level_style.value(record.level()),
                    style.value("]["),
                    record.target(),
                    style.value(']'),
                    record.args()
                )
            }
        });
        builder.init();
    });
}

#[cfg(test)]
mod test {
    use super::Runtime;
    use std::net::SocketAddr;

    #[test]
    // TODO: fix and enable this
    //#[should_panic]
    fn block_on_panics_in_runtime() {
        let runtime = Runtime::new();
        let addr1 = "10.0.0.1:1".parse::<SocketAddr>().unwrap();
        let node1 = runtime.create_node().ip(addr1.ip()).build();

        let f = node1.spawn(async move {
            let rt2 = Runtime::new();
            rt2.block_on(async { });
        });

        runtime.block_on(f).unwrap();
    }
}
