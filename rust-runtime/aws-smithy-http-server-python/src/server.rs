/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::TcpListener as StdTcpListener;
use std::ops::Deref;
use std::process;
use std::sync::{mpsc, Arc};
use std::thread;

use aws_smithy_http_server::{
    body::{Body, BoxBody},
    routing::IntoMakeService,
};
use http::{Request, Response};
use hyper::server::conn::AddrIncoming;
use parking_lot::Mutex;
use pyo3::{prelude::*, types::IntoPyDict};
use signal_hook::{consts::*, iterator::Signals};
use socket2::Socket;
use tokio::{net::TcpListener, runtime};
use tokio_rustls::TlsAcceptor;
use tower::{util::BoxCloneService, ServiceBuilder};

use crate::{
    context::{layer::AddPyContextLayer, PyContext},
    tls::{listener::Listener as TlsListener, PyTlsConfig},
    util::{error::rich_py_err, func_metadata},
    PySocket,
};

/// A Python handler function representation.
///
/// The Python business logic implementation needs to carry some information
/// to be executed properly like the size of its arguments and if it is
/// a coroutine.
#[pyclass]
#[derive(Debug, Clone)]
pub struct PyHandler {
    pub func: PyObject,
    // Number of args is needed to decide whether handler accepts context as an argument
    pub args: usize,
    pub is_coroutine: bool,
}

impl Deref for PyHandler {
    type Target = PyObject;

    fn deref(&self) -> &Self::Target {
        &self.func
    }
}

// A `BoxCloneService` with default `Request`, `Response` and `Error`.
type Service = BoxCloneService<Request<Body>, Response<BoxBody>, Infallible>;

/// Trait defining a Python application.
///
/// A Python application requires handling of multiple processes, signals and allows to register Python
/// function that will be executed as business logic by the code generated Rust handlers.
/// To properly function, the application requires some state:
/// * `workers`: the list of child Python worker processes, protected by a Mutex.
/// * `context`: the optional Python object that should be passed inside the Rust state struct.
/// * `handlers`: the mapping between an operation name and its [PyHandler] representation.
///
/// Since the Python application is spawning multiple workers, it also requires signal handling to allow the gracefull
/// termination of multiple Hyper servers. The main Rust process is registering signal and using them to understand when it
/// it time to loop through all the active workers and terminate them. Workers registers their own signal handlers and attaches
/// them to the Python event loop, ensuring all coroutines are cancelled before terminating a worker.
///
/// This trait will be implemented by the code generated by the `PythonApplicationGenerator` Kotlin class.
pub trait PyApp: Clone + pyo3::IntoPy<PyObject> {
    /// List of active Python workers registered with this application.
    fn workers(&self) -> &Mutex<Vec<PyObject>>;

    /// Optional Python context object that will be passed as part of the Rust state.
    fn context(&self) -> &Option<PyObject>;

    /// Mapping between operation names and their `PyHandler` representation.
    fn handlers(&mut self) -> &mut HashMap<String, PyHandler>;

    /// Build the app's `Service` using given `event_loop`.
    fn build_service(&self, event_loop: &pyo3::PyAny) -> pyo3::PyResult<Service>;

    /// Handle the graceful termination of Python workers by looping through all the
    /// active workers and calling `terminate()` on them. If termination fails, this
    /// method will try to `kill()` any failed worker.
    fn graceful_termination(&self, workers: &Mutex<Vec<PyObject>>) -> ! {
        let workers = workers.lock();
        for (idx, worker) in workers.iter().enumerate() {
            let idx = idx + 1;
            Python::with_gil(|py| {
                let pid: isize = worker
                    .getattr(py, "pid")
                    .map(|pid| pid.extract(py).unwrap_or(-1))
                    .unwrap_or(-1);
                tracing::debug!(idx, pid, "terminating worker");
                match worker.call_method0(py, "terminate") {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!(error = ?rich_py_err(e), idx, pid, "error terminating worker");
                        worker
                            .call_method0(py, "kill")
                            .map_err(|e| {
                                tracing::error!(
                                    error = ?rich_py_err(e), idx, pid, "unable to kill kill worker"
                                );
                            })
                            .unwrap();
                    }
                }
            });
        }
        process::exit(0);
    }

    /// Handler the immediate termination of Python workers by looping through all the
    /// active workers and calling `kill()` on them.
    fn immediate_termination(&self, workers: &Mutex<Vec<PyObject>>) -> ! {
        let workers = workers.lock();
        for (idx, worker) in workers.iter().enumerate() {
            let idx = idx + 1;
            Python::with_gil(|py| {
                let pid: isize = worker
                    .getattr(py, "pid")
                    .map(|pid| pid.extract(py).unwrap_or(-1))
                    .unwrap_or(-1);
                tracing::debug!(idx, pid, "killing worker");
                worker
                    .call_method0(py, "kill")
                    .map_err(|e| {
                        tracing::error!(error = ?rich_py_err(e), idx, pid, "unable to kill kill worker");
                    })
                    .unwrap();
            });
        }
        process::exit(0);
    }

    /// Register and handler signals of the main Rust thread. Signals not registered
    /// in this method are ignored.
    ///
    /// Signals supported:
    ///   * SIGTERM|SIGQUIT - graceful termination of all workers.
    ///   * SIGINT - immediate termination of all workers.
    ///
    /// Other signals are NOOP.
    fn block_on_rust_signals(&self) {
        let mut signals =
            Signals::new([SIGINT, SIGHUP, SIGQUIT, SIGTERM, SIGUSR1, SIGUSR2, SIGWINCH])
                .expect("Unable to register signals");
        for sig in signals.forever() {
            match sig {
                SIGINT => {
                    tracing::info!(
                        sig = %sig, "termination signal received, all workers will be immediately terminated"
                    );

                    self.immediate_termination(self.workers());
                }
                SIGTERM | SIGQUIT => {
                    tracing::info!(
                        sig = %sig, "termination signal received, all workers will be gracefully terminated"
                    );
                    self.graceful_termination(self.workers());
                }
                _ => {
                    tracing::debug!(sig = %sig, "signal is ignored by this application");
                }
            }
        }
    }

    /// Register and handle termination of all the tasks on the Python asynchronous event loop.
    /// We only register SIGQUIT and SIGINT since the main signal handling is done by Rust.
    fn register_python_signals(&self, py: Python, event_loop: PyObject) -> PyResult<()> {
        let locals = [("event_loop", event_loop)].into_py_dict(py);
        py.run(
            r#"
import asyncio
import logging
import functools
import signal

async def shutdown(sig, event_loop):
    # reimport asyncio and logging to be sure they are available when
    # this handler runs on signal catching.
    import asyncio
    import logging
    logging.info(f"Caught signal {sig.name}, cancelling tasks registered on this loop")
    tasks = [task for task in asyncio.all_tasks() if task is not
             asyncio.current_task()]
    list(map(lambda task: task.cancel(), tasks))
    results = await asyncio.gather(*tasks, return_exceptions=True)
    logging.debug(f"Finished awaiting cancelled tasks, results: {results}")
    event_loop.stop()

event_loop.add_signal_handler(signal.SIGTERM,
    functools.partial(asyncio.ensure_future, shutdown(signal.SIGTERM, event_loop)))
event_loop.add_signal_handler(signal.SIGINT,
    functools.partial(asyncio.ensure_future, shutdown(signal.SIGINT, event_loop)))
"#,
            None,
            Some(locals),
        )?;
        Ok(())
    }

    /// Start a single worker with its own Tokio and Python async runtime and provided shared socket.
    ///
    /// Python asynchronous loop needs to be started and handled during the lifetime of the process and
    /// it is passed to this method by the caller, which can use
    /// [configure_python_event_loop](#method.configure_python_event_loop) to properly setup it up.
    ///
    /// We retrieve the Python context object, if setup by the user calling [PyApp::context] method,
    /// generate the state structure and build the [aws_smithy_http_server::routing::Router], filling
    /// it with the functions generated by `PythonServerOperationHandlerGenerator.kt`.
    /// At last we get a cloned reference to the underlying [socket2::Socket].
    ///
    /// Now that all the setup is done, we can start the two runtimes and run the [hyper] server.
    /// We spawn a thread with a new [tokio::runtime], setup the middlewares and finally block the
    /// thread on Hyper serve() method.
    /// The main process continues and at the end it is blocked on Python `loop.run_forever()`.
    ///
    /// [uvloop]: https://github.com/MagicStack/uvloop
    fn start_hyper_worker(
        &mut self,
        py: Python,
        socket: &PyCell<PySocket>,
        event_loop: &PyAny,
        service: Service,
        worker_number: isize,
        tls: Option<PyTlsConfig>,
    ) -> PyResult<()> {
        // Clone the socket.
        let borrow = socket.try_borrow_mut()?;
        let held_socket: &PySocket = &borrow;
        let raw_socket = held_socket.get_socket()?;

        // Register signals on the Python event loop.
        self.register_python_signals(py, event_loop.to_object(py))?;

        // Spawn a new background [std::thread] to run the application.
        // This is needed because `asyncio` doesn't work properly if it doesn't control the main thread.
        // At the end of this function you can see we are calling `event_loop.run_forever()` to
        // yield execution of main thread to `asyncio` runtime.
        // For more details: https://docs.rs/pyo3-asyncio/latest/pyo3_asyncio/#pythons-event-loop-and-the-main-thread
        tracing::trace!("start the tokio runtime in a background task");
        thread::spawn(move || {
            // The thread needs a new [tokio] runtime.
            let rt = runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name(format!("smithy-rs-tokio[{worker_number}]"))
                .build()
                .expect("unable to start a new tokio runtime for this process");
            rt.block_on(async move {
                let addr = addr_incoming_from_socket(raw_socket);

                if let Some(config) = tls {
                    let (acceptor, acceptor_rx) = tls_config_reloader(config);
                    let listener = TlsListener::new(acceptor, addr, acceptor_rx);
                    let server =
                        hyper::Server::builder(listener).serve(IntoMakeService::new(service));

                    tracing::trace!("started tls hyper server from shared socket");
                    // Run forever-ish...
                    if let Err(err) = server.await {
                        tracing::error!(error = ?err, "server error");
                    }
                } else {
                    let server = hyper::Server::builder(addr).serve(IntoMakeService::new(service));

                    tracing::trace!("started hyper server from shared socket");
                    // Run forever-ish...
                    if let Err(err) = server.await {
                        tracing::error!(error = ?err, "server error");
                    }
                }
            });
        });
        // Block on the event loop forever.
        tracing::trace!("run and block on the python event loop until a signal is received");
        event_loop.call_method0("run_forever")?;
        Ok(())
    }

    /// Register a Python function to be executed inside the Smithy Rust handler.
    ///
    /// There are some information needed to execute the Python code from a Rust handler,
    /// such has if the registered function needs to be awaited (if it is a coroutine) and
    /// the number of arguments available, which tells us if the handler wants the state to be
    /// passed or not.
    fn register_operation(&mut self, py: Python, name: &str, func: PyObject) -> PyResult<()> {
        let func_metadata = func_metadata(py, &func)?;
        let handler = PyHandler {
            func,
            is_coroutine: func_metadata.is_coroutine,
            args: func_metadata.num_args,
        };
        tracing::info!(
            name,
            is_coroutine = handler.is_coroutine,
            args = handler.args,
            "registering handler function",
        );
        // Insert the handler in the handlers map.
        self.handlers().insert(name.to_string(), handler);
        Ok(())
    }

    /// Configure the Python asyncio event loop.
    ///
    /// First of all we install [uvloop] as the main Python event loop. Thanks to libuv, uvloop
    /// performs ~20% better than Python standard event loop in most benchmarks, while being 100%
    /// compatible. If [uvloop] is not available as a dependency, we just fall back to the standard
    /// Python event loop.
    ///
    /// [uvloop]: https://github.com/MagicStack/uvloop
    fn configure_python_event_loop<'py>(&self, py: Python<'py>) -> PyResult<&'py PyAny> {
        let asyncio = py.import("asyncio")?;
        match py.import("uvloop") {
            Ok(uvloop) => {
                uvloop.call_method0("install")?;
                tracing::trace!("setting up uvloop for current process");
            }
            Err(_) => {
                tracing::warn!("uvloop not found, using python standard event loop, which could have worse performance than uvloop");
            }
        }
        let event_loop = asyncio.call_method0("new_event_loop")?;
        asyncio.call_method1("set_event_loop", (event_loop,))?;
        Ok(event_loop)
    }

    /// Main entrypoint: start the server on multiple workers.
    ///
    /// The multiprocessing server is achieved using the ability of a Python interpreter
    /// to clone and start itself as a new process.
    /// The shared sockets is created and Using the [multiprocessing::Process] module, multiple
    /// workers with the method `self.start_worker()` as target are started.
    ///
    /// NOTE: this method ends up calling `self.start_worker` from the Python context, forcing
    /// the struct implementing this trait to also implement a `start_worker` method.
    /// This is done to ensure the Python event loop is started in the right child process space before being
    /// passed to `start_hyper_worker`.
    ///
    /// `PythonApplicationGenerator.kt` generates the `start_worker` method:
    ///
    /// ```no_run
    ///     use std::convert::Infallible;
    ///     use std::collections::HashMap;
    ///     use pyo3::prelude::*;
    ///     use aws_smithy_http_server_python::{PyApp, PyHandler};
    ///     use aws_smithy_http_server::body::{Body, BoxBody};
    ///     use parking_lot::Mutex;
    ///     use http::{Request, Response};
    ///     use tower::util::BoxCloneService;
    ///
    ///     #[pyclass]
    ///     #[derive(Debug, Clone)]
    ///     pub struct App {};
    ///
    ///     impl PyApp for App {
    ///         fn workers(&self) -> &Mutex<Vec<PyObject>> { todo!() }
    ///         fn context(&self) -> &Option<PyObject> { todo!() }
    ///         fn handlers(&mut self) -> &mut HashMap<String, PyHandler> { todo!() }
    ///         fn build_service(&mut self, event_loop: &PyAny) -> PyResult<BoxCloneService<Request<Body>, Response<BoxBody>, Infallible>> { todo!() }
    ///     }
    ///
    ///     #[pymethods]
    ///     impl App {
    ///         #[pyo3(text_signature = "($self, socket, worker_number, tls)")]
    ///         pub fn start_worker(
    ///             &mut self,
    ///             py: pyo3::Python,
    ///             socket: &pyo3::PyCell<aws_smithy_http_server_python::PySocket>,
    ///             worker_number: isize,
    ///             tls: Option<aws_smithy_http_server_python::tls::PyTlsConfig>,
    ///         ) -> pyo3::PyResult<()> {
    ///             let event_loop = self.configure_python_event_loop(py)?;
    ///             let service = self.build_service(event_loop)?;
    ///             self.start_hyper_worker(py, socket, event_loop, service, worker_number, tls)
    ///         }
    ///     }
    /// ```
    ///
    /// [multiprocessing::Process]: https://docs.python.org/3/library/multiprocessing.html
    fn run_server(
        &mut self,
        py: Python,
        address: Option<String>,
        port: Option<i32>,
        backlog: Option<i32>,
        workers: Option<usize>,
        tls: Option<PyTlsConfig>,
    ) -> PyResult<()> {
        // Setup multiprocessing environment, allowing connections and socket
        // sharing between processes.
        let mp = py.import("multiprocessing")?;
        // https://github.com/python/cpython/blob/f4c03484da59049eb62a9bf7777b963e2267d187/Lib/multiprocessing/context.py#L164
        mp.call_method0("allow_connection_pickling")?;

        // Starting from Python 3.8, on macOS, the spawn start method is now the default. See bpo-33725.
        // This forces the `PyApp` class to be pickled when it is shared between different process,
        // which is currently not supported by PyO3 classes.
        //
        // Forcing the multiprocessing start method to fork is a workaround for it.
        // https://github.com/pytest-dev/pytest-flask/issues/104#issuecomment-577908228
        #[cfg(target_os = "macos")]
        mp.call_method(
            "set_start_method",
            ("fork",),
            // We need to pass `force=True` to prevent `context has already been set` exception,
            // see https://github.com/pytorch/pytorch/issues/3492
            Some(vec![("force", true)].into_py_dict(py)),
        )?;

        let address = address.unwrap_or_else(|| String::from("127.0.0.1"));
        let port = port.unwrap_or(13734);
        let socket = PySocket::new(address, port, backlog)?;
        // Lock the workers mutex.
        let mut active_workers = self.workers().lock();
        // Register the main signal handler.
        // TODO(move from num_cpus to thread::available_parallelism after MSRV is 1.60)
        // Start all the workers as new Python processes and store the in the `workers` attribute.
        for idx in 1..workers.unwrap_or_else(num_cpus::get) + 1 {
            let sock = socket.try_clone()?;
            let tls = tls.clone();
            let process = mp.getattr("Process")?;
            let handle = process.call1((
                py.None(),
                self.clone().into_py(py).getattr(py, "start_worker")?,
                format!("smithy-rs-worker[{idx}]"),
                (sock.into_py(py), idx, tls.into_py(py)),
            ))?;
            handle.call_method0("start")?;
            active_workers.push(handle.to_object(py));
        }
        // Unlock the workers mutex.
        drop(active_workers);
        tracing::trace!("rust python server started successfully");
        self.block_on_rust_signals();
        Ok(())
    }

    /// Lambda main entrypoint: start the handler on Lambda.
    fn run_lambda_handler(&mut self, py: Python) -> PyResult<()> {
        use aws_smithy_http_server::routing::LambdaHandler;

        let event_loop = self.configure_python_event_loop(py)?;
        // Register signals on the Python event loop.
        self.register_python_signals(py, event_loop.to_object(py))?;

        let service = self.build_and_configure_service(py, event_loop)?;

        // Spawn a new background [std::thread] to run the application.
        // This is needed because `asyncio` doesn't work properly if it doesn't control the main thread.
        // At the end of this function you can see we are calling `event_loop.run_forever()` to
        // yield execution of main thread to `asyncio` runtime.
        // For more details: https://docs.rs/pyo3-asyncio/latest/pyo3_asyncio/#pythons-event-loop-and-the-main-thread
        tracing::trace!("start the tokio runtime in a background task");
        thread::spawn(move || {
            let rt = runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("unable to start a new tokio runtime for this process");
            rt.block_on(async move {
                let handler = LambdaHandler::new(service);
                let lambda = lambda_http::run(handler);
                tracing::debug!("starting lambda handler");
                if let Err(err) = lambda.await {
                    tracing::error!(error = %err, "unable to start lambda handler");
                }
            });
        });
        // Block on the event loop forever.
        tracing::trace!("run and block on the python event loop until a signal is received");
        event_loop.call_method0("run_forever")?;
        Ok(())
    }

    // Builds the `Service` and adds necessary layers to it.
    fn build_and_configure_service(
        &mut self,
        py: Python,
        event_loop: &pyo3::PyAny,
    ) -> pyo3::PyResult<Service> {
        let service = self.build_service(event_loop)?;
        let context = PyContext::new(self.context().clone().unwrap_or_else(|| py.None()))?;
        let service = ServiceBuilder::new()
            .boxed_clone()
            .layer(AddPyContextLayer::new(context))
            .service(service);
        Ok(service)
    }
}

fn addr_incoming_from_socket(socket: Socket) -> AddrIncoming {
    let std_listener: StdTcpListener = socket
        .try_into()
        .expect("unable to convert `socket2::Socket` into `std::net::TcpListener`");
    // StdTcpListener::from_std doesn't set O_NONBLOCK
    std_listener
        .set_nonblocking(true)
        .expect("unable to set `O_NONBLOCK=true` on `std::net::TcpListener`");
    let listener = TcpListener::from_std(std_listener)
        .expect("unable to create `tokio::net::TcpListener` from `std::net::TcpListener`");
    AddrIncoming::from_listener(listener)
        .expect("unable to create `AddrIncoming` from `TcpListener`")
}

// Builds `TlsAcceptor` from given `config` and also creates a background task
// to reload certificates and returns a channel to receive new `TlsAcceptor`s.
fn tls_config_reloader(config: PyTlsConfig) -> (TlsAcceptor, mpsc::Receiver<TlsAcceptor>) {
    let reload_dur = config.reload_duration();
    let (tx, rx) = mpsc::channel();
    let acceptor = TlsAcceptor::from(Arc::new(config.build().expect("invalid tls config")));

    tokio::spawn(async move {
        tracing::trace!(dur = ?reload_dur, "starting timer to reload tls config");
        loop {
            tokio::time::sleep(reload_dur).await;
            tracing::trace!("reloading tls config");
            match config.build() {
                Ok(config) => {
                    let new_config = TlsAcceptor::from(Arc::new(config));
                    // Note on expect: `tx.send` can only fail if the receiver is dropped,
                    // it probably a bug if that happens
                    tx.send(new_config).expect("could not send new tls config")
                }
                Err(err) => {
                    tracing::error!(error = ?err, "could not reload tls config because it is invalid");
                }
            }
        }
    });

    (acceptor, rx)
}
