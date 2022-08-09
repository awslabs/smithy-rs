/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */
// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.

use std::{process, sync::Arc, thread};

use aws_smithy_http_server::{AddExtensionLayer, Router};
use parking_lot::Mutex;
use pyo3::{prelude::*, types::IntoPyDict};
use signal_hook::{consts::*, iterator::Signals};
use tokio::runtime;
use tower::ServiceBuilder;

use crate::{PyHandler, PyHandlers, PySocket, PyState};

/// Python compatible wrapper for the [aws_smithy_http_server::Router] type.
#[pyclass(text_signature = "(router)")]
#[derive(Debug, Clone)]
pub struct PyRouter(pub Router);

/// Python application definition, holding the handlers map, the optional Python context object,
/// the list of workers and the [PyRouter].
#[pyclass(subclass, text_signature = "()")]
#[derive(Debug, Default)]
pub struct PyApp {
    pub handlers: PyHandlers,
    pub context: Option<Arc<PyObject>>,
    pub workers: Mutex<Vec<PyObject>>,
    pub router: Option<PyRouter>,
}

impl Clone for PyApp {
    fn clone(&self) -> Self {
        Self {
            handlers: self.handlers.clone(),
            context: self.context.clone(),
            workers: Mutex::new(vec![]),
            router: self.router.clone(),
        }
    }
}

#[allow(dead_code)]
impl PyApp {
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
                tracing::debug!("Terminating worker {idx}, PID: {pid}");
                match worker.call_method0(py, "terminate") {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("Error terminating worker {idx}, PID: {pid}: {e}");
                        worker
                            .call_method0(py, "kill")
                            .map_err(|e| {
                                tracing::error!(
                                    "Unable to kill kill worker {idx}, PID: {pid}: {e}"
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
                tracing::debug!("Killing worker {idx}, PID: {pid}");
                worker
                    .call_method0(py, "kill")
                    .map_err(|e| {
                        tracing::error!("Unable to kill kill worker {idx}, PID: {pid}: {e}");
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
            Signals::new(&[SIGINT, SIGHUP, SIGQUIT, SIGTERM, SIGUSR1, SIGUSR2, SIGWINCH])
                .expect("Unable to register signals");
        for sig in signals.forever() {
            match sig {
                SIGINT => {
                    tracing::info!(
                        "Termination signal {sig:?} received, all workers will be immediately terminated"
                    );

                    self.immediate_termination(&self.workers);
                }
                SIGTERM | SIGQUIT => {
                    tracing::info!(
                        "Termination signal {sig:?} received, all workers will be gracefully terminated"
                    );
                    self.graceful_termination(&self.workers);
                }
                _ => {
                    tracing::warn!("Signal {sig:?} is ignored by this application");
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
}

#[pymethods]
impl PyApp {
    /// Register a new context object inside the Rust state.
    #[pyo3(text_signature = "($self, context)")]
    pub fn context(&mut self, _py: Python, context: PyObject) {
        self.context = Some(Arc::new(context));
    }

    /// Start a single worker with its own Tokio and Python async runtime and provided shared socket.
    ///
    /// Python asynchronous loop needs to be started and handled during the lifetime of the process.
    /// First of all we install [uvloop] as the main Python event loop. Thanks to libuv, uvloop
    /// performs ~20% better than Python standard event loop in most benchmarks, while being 100%
    /// compatible. If [uvloop] is not available as a dependency, we just fall back to the standard
    /// Python event loop.
    ///
    /// We retrieve the Python context object, if setup by the user calling [PyApp::context] method,
    /// generate the [PyState] structure and build the [aws_smithy_http_server::Router], filling
    /// it with the functions generated by `PythonServerOperationHandlerGenerator.kt`.
    /// At last we get a cloned reference to the underlying [socket2::Socket].
    ///
    /// Now that all the setup is done, we can start the two runtimes and run the [hyper] server.
    /// We spawn a thread with a new [tokio::runtime], setup the middlewares and finally block the
    /// thread on `hyper::serve`.
    /// The main process continues and at the end it is blocked on Python `loop.run_forever()`.
    ///
    /// [uvloop]: https://github.com/MagicStack/uvloop
    #[pyo3(text_signature = "($self, socket, worker_number)")]
    pub fn start_worker(
        &mut self,
        py: Python,
        socket: &PyCell<PySocket>,
        worker_number: isize,
    ) -> PyResult<()> {
        // Setup the Python asyncio loop to use `uvloop`.
        // If uvloop is not available as a dependency, the standard Python
        // event loop will be used instead.
        let asyncio = py.import("asyncio")?;
        match py.import("uvloop") {
            Ok(uvloop) => {
                uvloop.call_method0("install")?;
                tracing::debug!("Setting up uvloop for current process");
            }
            Err(_) => {
                tracing::warn!("Uvloop not found, using Python standard event loop, which could have worse performance than uvloop");
            }
        }
        let event_loop = asyncio.call_method0("new_event_loop")?;
        asyncio.call_method1("set_event_loop", (event_loop,))?;
        // Create the `PyState` object from the Python context object.
        let context = self.context.clone().unwrap_or_else(|| Arc::new(py.None()));
        let state = PyState::new(context);
        // Build the router.
        let router: PyRouter = self.router.as_ref().expect("something").clone();
        // Clone the socket.
        let borrow = socket.try_borrow_mut()?;
        let held_socket: &PySocket = &*borrow;
        let raw_socket = held_socket.get_socket()?;
        // Register signals on the Python event loop.
        self.register_python_signals(py, event_loop.to_object(py))?;

        // Spawn a new background [std::thread] to run the application.
        tracing::debug!("Start the Tokio runtime in a background task");
        thread::spawn(move || {
            // The thread needs a new [tokio] runtime.
            let rt = runtime::Builder::new_multi_thread()
                .enable_all()
                .thread_name(format!("smithy-rs-tokio[{worker_number}]"))
                .build()
                .expect("Unable to start a new tokio runtime for this process");
            // Register operations into a Router, add middleware and start the `hyper` server,
            // all inside a [tokio] blocking function.
            rt.block_on(async move {
                tracing::debug!("Add middlewares to Rust Python router");
                let app = router
                    .0
                    .layer(ServiceBuilder::new().layer(AddExtensionLayer::new(state)));
                let server = hyper::Server::from_tcp(
                    raw_socket
                        .try_into()
                        .expect("Unable to convert socket2::Socket into std::net::TcpListener"),
                )
                .expect("Unable to create hyper server from shared socket")
                .serve(app.into_make_service());

                tracing::debug!("Started hyper server from shared socket");
                // Run forever-ish...
                if let Err(err) = server.await {
                    tracing::error!("server error: {}", err);
                }
            });
        });
        // Block on the event loop forever.
        tracing::debug!("Run and block on the Python event loop until a signal is received");
        event_loop.call_method0("run_forever")?;
        Ok(())
    }

    /// Register a new operation in the handlers map.
    ///
    /// The operation registered in the map are used inside the code-generated `router()` method
    /// and passed to the [aws_smithy_http_server::Router] as part of the operation handlers call.
    #[pyo3(text_signature = "($self, name, func)")]
    pub fn register_operation(&mut self, py: Python, name: &str, func: PyObject) -> PyResult<()> {
        let inspect = py.import("inspect")?;
        // Check if the function is a coroutine.
        // NOTE: that `asyncio.iscoroutine()` doesn't work here.
        let is_coroutine = inspect
            .call_method1("iscoroutinefunction", (&func,))?
            .extract::<bool>()?;
        // Find number of expected methods (a Pythzzon implementation could not accept the context).
        let func_args = inspect
            .call_method1("getargs", (func.getattr(py, "__code__")?,))?
            .getattr("args")?
            .extract::<Vec<String>>()?;
        let handler = PyHandler {
            func,
            is_coroutine,
            args: func_args.len(),
        };
        tracing::info!(
            "Registering function `{name}`, coroutine: {}, arguments: {}",
            handler.is_coroutine,
            handler.args,
        );
        // Insert the handler in the handlers map.
        self.handlers
            .inner
            .insert(String::from(name), Arc::new(handler));
        Ok(())
    }

    /// Main entrypoint: start the server on multiple workers.
    ///
    /// The multiprocessing server is achieved using the ability of a Python interpreter
    /// to clone and start itself as a new process.
    /// The shared sockets is created and Using the [multiprocessing::Process] module, multiple
    /// workers with the method `self.start_worker()` as target are started.
    ///
    /// [multiprocessing::Process]: https://docs.python.org/3/library/multiprocessing.html
    #[pyo3(text_signature = "($self, address, port, backlog, workers)")]
    pub fn run(
        &mut self,
        py: Python,
        address: Option<String>,
        port: Option<i32>,
        backlog: Option<i32>,
        workers: Option<usize>,
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
        mp.call_method1("set_start_method", ("fork",))?;

        let address = address.unwrap_or_else(|| String::from("127.0.0.1"));
        let port = port.unwrap_or(13734);
        let socket = PySocket::new(address, port, backlog)?;
        // Lock the workers mutex.
        let mut active_workers = self.workers.lock();
        // Register the main signal handler.
        // TODO(move from num_cpus to thread::available_parallelism after MSRV is 1.60)
        // Start all the workers as new Python processes and store the in the `workers` attribute.
        for idx in 1..workers.unwrap_or_else(num_cpus::get) + 1 {
            let sock = socket.try_clone()?;
            let process = mp.getattr("Process")?;
            let handle = process.call1((
                py.None(),
                self.clone().into_py(py).getattr(py, "start_worker")?,
                format!("smithy-rs-worker[{idx}]"),
                (sock.into_py(py), idx),
            ))?;
            handle.call_method0("start")?;
            active_workers.push(handle.to_object(py));
        }
        // Unlock the workers mutex.
        drop(active_workers);
        tracing::info!("Rust Python server started successfully");
        self.block_on_rust_signals();
        Ok(())
    }
}
