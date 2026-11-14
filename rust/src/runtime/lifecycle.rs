use super::*;

impl Runtime {
    pub(super) fn read_loop(self: &Arc<Self>, mut input: BufReader<io::Stdin>) {
        loop {
            match dispatcher::read_message(&mut input) {
                Ok(Some(message)) => {
                    if self.handle_message(message) {
                        break;
                    }
                }
                Ok(None) => {
                    self.request_shutdown(None);
                    break;
                }
                Err(error) => {
                    self.request_shutdown(Some(error.to_string()));
                    break;
                }
            }
        }
    }

    /// Returns true when the control channel should stop reading.
    fn handle_message(self: &Arc<Self>, message: Value) -> bool {
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            if let Some(id) = message.get("id").and_then(Value::as_u64) {
                self.dispatcher.complete(id, message);
            }
            return false;
        };
        let id = message.get("id").cloned();
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match method {
            "plugin.initialize" => {
                let version = params.get("protocolVersion").and_then(Value::as_str);
                if version != Some(PROTOCOL_VERSION) {
                    if let Some(id) = id {
                        self.dispatcher.reply(
                            id,
                            Err(RpcError::invalid_params("unsupported protocol version")),
                        );
                    }
                    self.request_shutdown(Some("unsupported protocol version".into()));
                    return true;
                }
                self.initialized.store(true, Ordering::Release);
                if let Some(id) = id {
                    let mut reply = json!({
                        "pluginId": self.plugin_id,
                        "protocolVersion": PROTOCOL_VERSION,
                        "sdkVersion": env!("CARGO_PKG_VERSION"),
                        "ready": true,
                        "concurrentRequests": true,
                        "tasks": true,
                    });
                    if let (Some(target), Some(extra)) =
                        (reply.as_object_mut(), self.capabilities.as_object())
                    {
                        for (key, value) in extra {
                            target.insert(key.clone(), value.clone());
                        }
                    }
                    self.dispatcher.reply(id, Ok(reply));
                }
                false
            }
            "plugin.ping" => {
                if let Some(id) = id {
                    let result = if self.initialized.load(Ordering::Acquire) {
                        Ok(json!({"alive": true}))
                    } else {
                        Err(RpcError::invalid_params("initialize required"))
                    };
                    self.dispatcher.reply(id, result);
                }
                false
            }
            "plugin.shutdown" => {
                if let Some(id) = id {
                    self.dispatcher.reply(id, Ok(Value::Null));
                }
                self.request_shutdown(None);
                true
            }
            "plugin.cancel" => {
                if let Some(task) = params.get("taskId").and_then(Value::as_str) {
                    self.tasks.cancel(task);
                }
                false
            }
            _ => {
                if !self.initialized.load(Ordering::Acquire) {
                    if let Some(id) = id {
                        self.dispatcher
                            .reply(id, Err(RpcError::invalid_params("initialize required")));
                    }
                    return false;
                }
                if let Some(id) = message.get("id").and_then(Value::as_u64) {
                    let locale = crate::i18n::Locale::parse(
                        message
                            .get("locale")
                            .and_then(Value::as_str)
                            .unwrap_or("zh-CN"),
                    );
                    self.spawn_request(id, method, params, locale);
                }
                false
            }
        }
    }

    fn spawn_request(
        self: &Arc<Self>,
        id: u64,
        method: &str,
        params: Value,
        locale: crate::i18n::Locale,
    ) {
        {
            let Ok(mut active) = self.active_requests.lock() else {
                return;
            };
            if *active >= MAX_REQUESTS {
                self.dispatcher.reply(
                    json!(id),
                    Err(RpcError::internal("too many concurrent requests")),
                );
                return;
            }
            *active += 1;
        }
        let runtime = Arc::clone(self);
        let method = method.to_string();
        let spawned = thread::Builder::new()
            .name(format!("fiex-sdk-request-{id}"))
            .spawn(move || {
                let context = runtime.context(session::Session::Operation(id));
                let result = catch_unwind(AssertUnwindSafe(|| {
                    crate::i18n::with_locale(locale, || {
                        (runtime.dispatch)(RequestContext::new(context, method, params))
                    })
                }))
                .unwrap_or_else(|_| Err(RpcError::internal("request handler panicked")));
                runtime.dispatcher.reply(json!(id), result);
                runtime.request_finished();
            });
        if let Err(error) = spawned {
            self.dispatcher.reply(
                json!(id),
                Err(RpcError::internal(format!(
                    "could not start request worker: {error}"
                ))),
            );
            self.request_finished();
        }
    }

    fn context(self: &Arc<Self>, session: session::Session) -> ContextInner {
        let link: Arc<dyn HostLink> = self.clone();
        ContextInner::new(
            link,
            Arc::clone(&self.tasks),
            session,
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn request_finished(&self) {
        if let Ok(mut active) = self.active_requests.lock() {
            *active = active.saturating_sub(1);
        }
        self.active_changed.notify_all();
    }

    fn request_shutdown(&self, error: Option<String>) {
        if let Ok(mut state) = self.shutdown.lock()
            && !state.requested
        {
            state.requested = true;
            state.error = error;
        }
        self.shutdown_changed.notify_all();
    }
}
