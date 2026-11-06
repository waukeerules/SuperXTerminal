//! Rhai scripts (<https://rhai.rs>): a second script language beside SPSL, for scripts that need real variables,
//! arithmetic, functions and loops: the Script Editor's Run as > Rhai script, and a button's Run Rhai script.
//!
//! A script runs on its own thread here in the core, since Rhai's functions must return their answer before the
//! script goes on, and a wait can take seconds. What it asks of its tab (type this, wait for that) goes to the tab's
//! page as a [`RhaiEvent::Call`] on the run's `Channel`, and the page answers with `rhai_reply`
//! ([`RhaiRuns::reply`]). The page does the work with what SPSL already uses (`src/lib/scripting/rhai.ts`, through
//! `PageHost`): where a wait starts looking, the cursor's line, and stopping when the session drops or reconnects.
//!
//! Its functions: `send`, `send_line`, `wait_for`, `wait_for_any`, `wait_prompt`, `matched`, `prompt`, `password`,
//! `status` and `sleep` (docs/RHAI.md), with a `wait_for` options map
//! (`#{ timeout: 10, regex: true, any_case: true }`), the tab's session's values as `session` (`session.host_address`),
//! and Environment Variables as `env`
//! (`env.ad_user`, as SPSL's `{ad_user}`): filled here in the core, since a secret's value never goes to the window,
//! seen inside the script's own functions too, and a name with no value stops the script saying which and why rather
//! than typing nothing. Stop sets a flag that `on_progress` checks between operations, so even a loop that never waits
//! ends at once.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, NativeCallContext, Position, Scope};
use serde::Serialize;
use serde_json::{Value, json};

/// How long a wait waits when the script doesn't say, as SPSL's WAITFOR (30 s)
const DEFAULT_WAIT_SECS: i64 = 30;

/// How often a script waiting for its page's answer, or sleeping, looks at Stop
const STOP_POLL: Duration = Duration::from_millis(50);

/// What a running script tells its tab's page, on the run's `Channel`
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RhaiEvent {
    /// Do something in the tab and answer with `rhai_reply` and this `id`. `line` is the script's line making the
    /// call, for the status bar. `call` is `type`, `wait`, `ask`; `args` its arguments (see `rhai.ts`).
    Call { id: u64, line: usize, call: String, args: Value },
    /// A line for the status bar (`status` and `print`); nothing to answer
    Status { line: usize, text: String },
    /// The script ended: `stopped` by Stop, or `error` (with the `line` it failed at, 0 when not known); neither when
    /// it ran to its end
    Done { stopped: bool, error: Option<String>, line: usize },
}

/// The page's answer to a [`RhaiEvent::Call`]: what it returned, or why it couldn't (the session ended...)
#[derive(Debug)]
pub struct Reply {
    id: u64,
    outcome: Result<Value, String>,
}

/// A running script, as the core keeps it: its Stop and where the page's answers go
struct Handle {
    abort: Arc<AtomicBool>,
    replies: Sender<Reply>,
}

/// The scripts running now, by run id: managed state, so `rhai_reply` and `rhai_stop` find theirs
#[derive(Default)]
pub struct RhaiRuns {
    runs: Mutex<HashMap<u64, Handle>>,
    next: AtomicU64,
}

impl RhaiRuns {
    /// Start a script; `send` takes its events to the page and says false when the page has gone (its window
    /// closed), which stops it. `on_end` runs once it has ended, after its [`RhaiEvent::Done`]. Its run id.
    pub fn start(
        &self,
        source: String,
        session: BTreeMap<String, String>,
        env: HashMap<String, String>,
        send: impl Fn(RhaiEvent) -> bool + Send + 'static,
        on_end: impl FnOnce(u64) + Send + 'static,
    ) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let (replies, inbox) = channel();
        let abort = Arc::new(AtomicBool::new(false));
        self.runs.lock().unwrap().insert(id, Handle { abort: abort.clone(), replies });
        std::thread::spawn(move || {
            let send: Rc<dyn Fn(RhaiEvent) -> bool> = Rc::new(send);
            let done = run(&source, session, env, send.clone(), inbox, abort);
            send(done);
            on_end(id);
        });
        id
    }

    /// The page's answer to a call; ignored once the script has ended (a reply that comes after Stop)
    pub fn reply(&self, run: u64, id: u64, outcome: Result<Value, String>) {
        if let Some(handle) = self.runs.lock().unwrap().get(&run) {
            let _ = handle.replies.send(Reply { id, outcome });
        }
    }

    /// Stop a script: a wait, sleep or loop ends at once
    pub fn stop(&self, run: u64) {
        if let Some(handle) = self.runs.lock().unwrap().get(&run) {
            handle.abort.store(true, Ordering::Relaxed);
        }
    }

    /// Forget a script that has ended
    pub fn remove(&self, run: u64) {
        self.runs.lock().unwrap().remove(&run);
    }
}

/// A problem the editor shows before a script runs: a syntax error and its line
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Issue {
    pub line: usize,
    pub message: String,
}

/// A script's first syntax error, or a variable used that was never set (a misspelt name, which Rhai would otherwise
/// only find when that line runs, after the lines before it have typed into the device). Functions are only looked up
/// when it runs, so a misspelt one isn't found here.
pub fn check(source: &str) -> Vec<Issue> {
    let (engine, scope) = configured(Engine::new(), BTreeMap::new(), HashMap::new());
    match engine.compile_with_scope(&scope, &without_marker(source)) {
        Ok(_) => Vec::new(),
        Err(error) => vec![Issue { line: error.1.line().unwrap_or(0), message: error.0.to_string() }],
    }
}

/// A script with its `#!` first line (`#!/bin/rhai`, which marks a session's or profile's login script as Rhai, as
/// `#!/bin/spsl` does an SPSL one) read as a comment: Rhai itself refuses `#!` ("a reserved symbol"). The line stays,
/// so the script's line numbers are as the editor shows them
fn without_marker(source: &str) -> std::borrow::Cow<'_, str> {
    match source.strip_prefix("#!") {
        Some(rest) => format!("//{rest}").into(),
        None => source.into(),
    }
}

/// The tab's session's values a script reads as `session.name`, as SPSL's `{name}` (spsl.ts SESSION_VARIABLES, which
/// sending.svelte.ts's sessionValues fills)
const SESSION_NAMES: [&str; 8] =
    ["host_address", "host_port", "session_name", "session_id", "session_folder", "session_username", "session_profile", "session_protocol"];

/// Which of a script's two sets of values a [`Values`] is, for what its messages say
#[derive(Clone, Copy, PartialEq)]
enum Source {
    /// `session`: the tab's session's values
    Session,
    /// `env`: Environment Variables
    Env,
}

/// The tab's session's values (`session`) or Environment Variables (`env`), as a script reads them: `session.name`
/// or `session["name"]`, by name in any case (both are kept in lower case). Not a plain map, so a name with no value
/// stops the script saying which and why, as SPSL offers to define a missing `{key}`, rather than giving `()`, which
/// would type nothing (`ssh ${session.host_address}` in a Local Shell tab) or fail later as a confusing "function
/// not found". `has("name")` asks without stopping.
#[derive(Clone)]
struct Values {
    source: Source,
    values: Rc<HashMap<String, String>>,
}

impl Values {
    /// A value by name, or why there's none: the name mistaken for the other set's (`env.host_address`), a session
    /// value this tab hasn't (a Local Shell), a name `session` never has, or an Environment Variable with no value
    fn get(&self, name: &str) -> Result<String, Box<EvalAltResult>> {
        let key = name.to_lowercase();
        if let Some(value) = self.values.get(&key) {
            return Ok(value.clone());
        }
        let session_name = SESSION_NAMES.contains(&key.as_str());
        Err(fail(match (self.source, session_name) {
            (Source::Env, true) => format!("env.{name}: {key} is the tab's session's, not an Environment Variable: write session.{key}"),
            (Source::Env, false) => format!("the Environment Variable {name} has no value: define it in Environment Variables"),
            (Source::Session, true) => {
                format!("this tab has no session.{key}: it isn't a saved session's (a Local Shell tab, or Quick connect without it)")
            }
            (Source::Session, false) => format!("session has no {name}: its values are {}", SESSION_NAMES.join(", ")),
        }))
    }
}

/// Teach the engine [`Values`]: its indexer (which `session.name` falls back to, a property being looked up by its
/// name) and `has("name")`
fn register_values(engine: &mut Engine) {
    engine.register_type_with_name::<Values>("Values");
    engine.register_indexer_get(|values: &mut Values, name: &str| values.get(name));
    engine.register_fn("has", |values: &mut Values, name: &str| values.values.contains_key(&name.to_lowercase()));
}

/// The engine as scripts run in it, and the scope they start with, with `session` and `env` (these values, or none
/// for Check script).
///
/// `session` and `env` are in the scope, which is what strict variables checks a script against as it's compiled, and
/// also come from `on_var`, so the script's own functions (`fn`) see them as it runs: a function sees only its
/// arguments otherwise. (Rhai copies a constant map into functions when it optimizes, so a plain map would only seem
/// to work there.) Strict variables: a variable must be set before it's used, so a misspelt
/// one stops the script before it starts, not halfway through. And limits on what a script can build: deep recursion
/// or a runaway string stops it with an error rather than taking the app's memory. No limit on operations, since a
/// script watching a device for hours is fine.
fn configured(mut engine: Engine, session: BTreeMap<String, String>, env: HashMap<String, String>) -> (Engine, Scope<'static>) {
    engine.set_strict_variables(true);
    register_values(&mut engine);
    let session = Values { source: Source::Session, values: Rc::new(session.into_iter().map(|(k, v)| (k.to_lowercase(), v)).collect()) };
    let env = Values { source: Source::Env, values: Rc::new(env) };
    let mut scope = Scope::new();
    scope.push_constant("session", session.clone());
    scope.push_constant("env", env.clone());
    // Rhai calls on_var "volatile" (it may change between versions), not deprecated: kept, and the tests below
    // (functions_see_session_and_env) say at once if an upgrade breaks it
    #[allow(deprecated)]
    engine.on_var(move |name, _, _| {
        Ok(match name {
            "session" => Some(Dynamic::from(session.clone())),
            "env" => Some(Dynamic::from(env.clone())),
            _ => None,
        })
    });
    engine.set_max_call_levels(64);
    engine.set_max_expr_depths(128, 64);
    engine.set_max_string_size(16 * 1024 * 1024);
    engine.set_max_array_size(1_000_000);
    engine.set_max_map_size(100_000);
    (engine, scope)
}

/// What the script's functions share on its thread: the way to the page, where answers come back, and Stop
struct Ctx {
    send: Rc<dyn Fn(RhaiEvent) -> bool>,
    replies: Receiver<Reply>,
    abort: Arc<AtomicBool>,
    next_call: Cell<u64>,
    /// What the last wait matched: the whole match, then a regular expression's groups (`matched()`)
    matched: RefCell<Array>,
}

/// A Rhai error from a message, at no position (Rhai puts the calling line's on it)
fn fail(message: impl Into<String>) -> Box<EvalAltResult> {
    message.into().into()
}

impl Ctx {
    /// Stop pressed: the script ends
    fn check(&self) -> Result<(), Box<EvalAltResult>> {
        if self.abort.load(Ordering::Relaxed) { Err(Box::new(EvalAltResult::ErrorTerminated(Dynamic::UNIT, Position::NONE))) } else { Ok(()) }
    }

    /// Ask the page to do something in the tab and wait for its answer, looking at Stop meanwhile
    fn call(&self, line: usize, call: &str, args: Value) -> Result<Value, Box<EvalAltResult>> {
        self.check()?;
        let id = self.next_call.get() + 1;
        self.next_call.set(id);
        if !(self.send)(RhaiEvent::Call { id, line, call: call.into(), args }) {
            return Err(fail("the tab closed"));
        }
        loop {
            self.check()?;
            match self.replies.recv_timeout(STOP_POLL) {
                Ok(reply) if reply.id == id => return reply.outcome.map_err(fail),
                // an answer to an earlier call that came late: not this one's
                Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(fail("the tab closed")),
            }
        }
    }

    /// Wait for one of several patterns; the index of the one found first, or None when the time ran out. What it
    /// matched is kept for `matched()`.
    fn wait(&self, line: usize, patterns: Vec<Value>, secs: i64) -> Result<Option<i64>, Box<EvalAltResult>> {
        // 0 (or less) waits for as long as it takes
        let timeout_ms = if secs > 0 { json!(secs * 1000) } else { Value::Null };
        let found = self.call(line, "wait", json!({ "patterns": patterns, "timeoutMs": timeout_ms }))?;
        let Some(index) = found.get("index").and_then(Value::as_i64) else { return Ok(None) };
        let groups = found.get("groups").and_then(Value::as_array).cloned().unwrap_or_default();
        *self.matched.borrow_mut() = groups.into_iter().map(|g| Dynamic::from(g.as_str().unwrap_or("").to_string())).collect();
        Ok(Some(index))
    }

    /// Sleep, ending early on Stop
    fn sleep(&self, ms: i64) -> Result<(), Box<EvalAltResult>> {
        let until = Instant::now() + Duration::from_millis(ms.max(0) as u64);
        loop {
            self.check()?;
            let now = Instant::now();
            if now >= until {
                return Ok(());
            }
            std::thread::sleep(STOP_POLL.min(until - now));
        }
    }
}

/// A pattern to wait for, as the page's `wait` takes it: text, or a regular expression, matched in any case or not
fn pattern(source: &str, regex: bool, any_case: bool) -> Value {
    json!({ "source": source, "regex": regex, "anyCase": any_case })
}

/// `wait_for`'s options map: `timeout` (seconds), `regex` and `any_case`; anything else is a mistake worth saying
fn wait_options(text: &str, options: &Map) -> Result<(Value, i64), Box<EvalAltResult>> {
    let mut secs = DEFAULT_WAIT_SECS;
    let (mut regex, mut any_case) = (false, false);
    for (key, value) in options {
        match key.as_str() {
            "timeout" => secs = value.as_int().map_err(|_| fail("wait_for: timeout is a whole number of seconds"))?,
            "regex" => regex = value.as_bool().map_err(|_| fail("wait_for: regex is true or false"))?,
            "any_case" => any_case = value.as_bool().map_err(|_| fail("wait_for: any_case is true or false"))?,
            other => return Err(fail(format!("wait_for: no option \"{other}\" (timeout, regex, any_case)"))),
        }
    }
    Ok((pattern(text, regex, any_case), secs))
}

/// The script's line a call comes from (1-based; 0 when Rhai doesn't know)
fn line_of(context: &NativeCallContext) -> usize {
    context.call_position().line().unwrap_or(0)
}

/// Run a script to its end on this thread; the [`RhaiEvent::Done`] to send
fn run(source: &str, session: BTreeMap<String, String>, env: HashMap<String, String>, send: Rc<dyn Fn(RhaiEvent) -> bool>, replies: Receiver<Reply>, abort: Arc<AtomicBool>) -> RhaiEvent {
    let ctx = Rc::new(Ctx { send, replies, abort: abort.clone(), next_call: Cell::new(0), matched: RefCell::default() });
    let result = {
        let (mut engine, mut scope) = configured(Engine::new(), session, env);
        {
            let stop = abort.clone();
            engine.on_progress(move |_| stop.load(Ordering::Relaxed).then_some(Dynamic::UNIT));
        }
        {
            let c = ctx.clone();
            engine.on_print(move |text| {
                (c.send)(RhaiEvent::Status { line: 0, text: text.to_string() });
            });
        }
        {
            let c = ctx.clone();
            engine.on_debug(move |text, _, at| {
                (c.send)(RhaiEvent::Status { line: at.line().unwrap_or(0), text: text.to_string() });
            });
        }

        /// Register a function whose closure gets the context (`c`) and the calling line (`line`)
        macro_rules! register {
            ($name:literal, |$c:ident, $line:ident $(, $arg:ident : $ty:ty)*| $body:expr) => {{
                let $c = ctx.clone();
                engine.register_fn($name, move |context: NativeCallContext $(, $arg: $ty)*| {
                    let $line = line_of(&context);
                    $body
                });
            }};
        }

        register!("send", |c, line, text: &str| c.call(line, "type", json!({ "text": text, "enter": false })).map(|_| ()));
        register!("send_line", |c, line, text: &str| c.call(line, "type", json!({ "text": text, "enter": true })).map(|_| ()));
        register!("wait_for", |c, line, text: &str| c.wait(line, vec![pattern(text, false, false)], DEFAULT_WAIT_SECS).map(|f| f.is_some()));
        register!("wait_for", |c, line, text: &str, secs: i64| c.wait(line, vec![pattern(text, false, false)], secs).map(|f| f.is_some()));
        register!("wait_for", |c, line, text: &str, options: Map| {
            let (look, secs) = wait_options(text, &options)?;
            c.wait(line, vec![look], secs).map(|f| f.is_some())
        });
        register!("wait_for_any", |c, line, texts: Array| c.wait(line, texts.iter().map(|t| pattern(&t.to_string(), false, false)).collect(), DEFAULT_WAIT_SECS).map(|f| f.unwrap_or(-1)));
        register!("wait_for_any", |c, line, texts: Array, secs: i64| c.wait(line, texts.iter().map(|t| pattern(&t.to_string(), false, false)).collect(), secs).map(|f| f.unwrap_or(-1)));
        register!("wait_prompt", |c, line| c.wait(line, vec![json!({ "prompt": true })], DEFAULT_WAIT_SECS).map(|f| f.is_some()));
        register!("wait_prompt", |c, line, secs: i64| c.wait(line, vec![json!({ "prompt": true })], secs).map(|f| f.is_some()));
        register!("matched", |c, _line| Ok::<Array, Box<EvalAltResult>>(c.matched.borrow().clone()));
        register!("sleep", |c, _line, ms: i64| c.sleep(ms));
        register!("status", |c, line, text: &str| {
            (c.send)(RhaiEvent::Status { line, text: text.to_string() });
            Ok::<(), Box<EvalAltResult>>(())
        });
        // Cancel gives "" for a question, as SPSL's PROMPT; a cancelled password stops the script, as PWDPROMPT
        register!("prompt", |c, line, text: &str| {
            c.call(line, "ask", json!({ "message": text, "secret": false })).map(|v| v.as_str().unwrap_or("").to_string())
        });
        register!("password", |c, line, text: &str| {
            match c.call(line, "ask", json!({ "message": text, "secret": true }))? {
                Value::String(answer) => Ok(answer),
                _ => Err(fail("you cancelled the password prompt")),
            }
        });

        engine.run_with_scope(&mut scope, &without_marker(source))
    };
    match result {
        Ok(()) => RhaiEvent::Done { stopped: false, error: None, line: 0 },
        Err(error) if matches!(*error, EvalAltResult::ErrorTerminated(..)) || abort.load(Ordering::Relaxed) => {
            RhaiEvent::Done { stopped: true, error: None, line: error.position().line().unwrap_or(0) }
        }
        Err(error) => {
            let line = error.position().line().unwrap_or(0);
            RhaiEvent::Done { stopped: false, error: Some(without_position(&error.to_string())), line }
        }
    }
}

/// A Rhai error's message as the status bar shows it: without the " (line 3, position 5)" it ends with, since the
/// status bar says the line itself, and without "Runtime error: " before a `throw`'s message or one of ours ("the
/// session ended"), which reads as the app failing rather than the script stopping, as SPSL's STOP
fn without_position(message: &str) -> String {
    let message = match message.rfind(" (line ") {
        Some(at) if message.ends_with(')') => &message[..at],
        _ => message,
    };
    message.strip_prefix("Runtime error: ").unwrap_or(message).to_string()
}

/// Run a script in a tab: its events come on `on_event`; its run id, for `rhai_reply` and `rhai_stop`. Its `env` is
/// Environment Variables as they are now, secrets from the Keychain, read here so they never pass through the window.
#[tauri::command]
pub fn rhai_run(app: tauri::AppHandle, source: String, session: BTreeMap<String, String>, on_event: tauri::ipc::Channel<RhaiEvent>) -> u64 {
    use tauri::Manager;
    let env = crate::variables::values(&app.state::<crate::AppState>().user_folder);
    let ended = app.clone();
    let runs = app.state::<RhaiRuns>();
    runs.start(source, session, env, move |event| on_event.send(event).is_ok(), move |id| ended.state::<RhaiRuns>().remove(id))
}

/// The page's answer to a script's call: `value`, or `error` when it couldn't do it (the session ended...)
#[tauri::command]
pub fn rhai_reply(runs: tauri::State<'_, RhaiRuns>, run: u64, id: u64, value: Option<Value>, error: Option<String>) {
    runs.reply(run, id, error.map_or(Ok(value.unwrap_or(Value::Null)), Err));
}

/// Stop a script (the status bar's Stop)
#[tauri::command]
pub fn rhai_stop(runs: tauri::State<'_, RhaiRuns>, run: u64) {
    runs.stop(run);
}

/// A script's syntax errors, for the Script Editor's Check script
#[tauri::command]
pub fn rhai_check(source: String) -> Vec<Issue> {
    check(&source)
}

#[cfg(test)]
mod tests {
    //! A script is run as the app runs one, on its own thread, with the page stood in for by this test: its events
    //! come here on a channel, and the test answers its calls as a tab would.

    use super::*;
    use std::path::Path;

    /// A script running with a stand-in page: its events, and its run id for answering
    struct Fake {
        runs: Arc<RhaiRuns>,
        id: u64,
        events: Receiver<RhaiEvent>,
    }

    impl Fake {
        /// Start a script with these session values
        fn start(source: &str, session: &[(&str, &str)]) -> Fake {
            Fake::start_with_env(source, session, &[])
        }

        /// Start a script with these session values and Environment Variables
        fn start_with_env(source: &str, session: &[(&str, &str)], env: &[(&str, &str)]) -> Fake {
            let runs = Arc::new(RhaiRuns::default());
            let (tx, events) = channel();
            let tx = Mutex::new(tx);
            let session = session.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let ended = runs.clone();
            let env = env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let id = runs.start(source.into(), session, env, move |e| tx.lock().unwrap().send(e).is_ok(), move |id| ended.remove(id));
            Fake { runs, id, events }
        }

        /// The script's next event
        fn next(&self) -> RhaiEvent {
            self.events.recv_timeout(Duration::from_secs(5)).expect("an event from the script")
        }

        /// The next event, which must be a call: its id, call and arguments
        fn call(&self) -> (u64, usize, String, Value) {
            match self.next() {
                RhaiEvent::Call { id, line, call, args } => (id, line, call, args),
                other => panic!("expected a call, got {other:?}"),
            }
        }

        /// Answer a call
        fn answer(&self, id: u64, outcome: Result<Value, String>) {
            self.runs.reply(self.id, id, outcome);
        }
    }

    #[test]
    fn a_script_types_waits_and_reads_what_matched_with_each_calls_line() {
        let fake = Fake::start("send_line(`ssh ${session.host_address}`);\nif wait_for(`(\\d+) ms`, #{ regex: true, timeout: 5 }) {\n  status(matched()[1]);\n}", &[("host_address", "10.0.0.1")]);

        let (id, line, call, args) = fake.call();
        assert_eq!((line, call.as_str()), (1, "type"));
        assert_eq!(args, json!({ "text": "ssh 10.0.0.1", "enter": true }));
        fake.answer(id, Ok(Value::Null));

        let (id, line, call, args) = fake.call();
        assert_eq!((line, call.as_str()), (2, "wait"));
        assert_eq!(args, json!({ "patterns": [{ "source": "(\\d+) ms", "regex": true, "anyCase": false }], "timeoutMs": 5000 }));
        fake.answer(id, Ok(json!({ "index": 0, "groups": ["12 ms", "12"] })));

        assert_eq!(fake.next(), RhaiEvent::Status { line: 3, text: "12".into() });
        assert_eq!(fake.next(), RhaiEvent::Done { stopped: false, error: None, line: 0 });
    }

    #[test]
    fn a_wait_that_times_out_lets_the_script_choose_and_wait_for_any_says_which() {
        let fake = Fake::start("let i = wait_for_any([\"#\", \">\"], 2);\nif !wait_for(\"x\") { status(`${i}`); }", &[]);
        let (id, _, _, args) = fake.call();
        assert_eq!(args["timeoutMs"], json!(2000));
        fake.answer(id, Ok(json!({ "index": 1, "groups": [">"] })));
        let (id, _, _, args) = fake.call();
        // the default, as SPSL's WAITFOR
        assert_eq!(args["timeoutMs"], json!(30_000));
        fake.answer(id, Ok(Value::Null));
        assert_eq!(fake.next(), RhaiEvent::Status { line: 2, text: "1".into() });
        assert!(matches!(fake.next(), RhaiEvent::Done { stopped: false, error: None, .. }));
    }

    #[test]
    fn stop_ends_a_wait_and_a_loop_that_never_waits() {
        let fake = Fake::start("wait_for(\"never\", 0);", &[]);
        let _ = fake.call();
        fake.runs.stop(fake.id);
        assert!(matches!(fake.next(), RhaiEvent::Done { stopped: true, error: None, .. }));

        let fake = Fake::start("let n = 0;\nloop { n += 1; }", &[]);
        std::thread::sleep(Duration::from_millis(100));
        fake.runs.stop(fake.id);
        assert!(matches!(fake.next(), RhaiEvent::Done { stopped: true, .. }));
    }

    #[test]
    fn the_page_saying_the_session_ended_stops_the_script_at_its_line() {
        let fake = Fake::start("status(\"start\");\nsend_line(\"show version\");\nstatus(\"never\");", &[]);
        assert!(matches!(fake.next(), RhaiEvent::Status { .. }));
        let (id, _, _, _) = fake.call();
        fake.answer(id, Err("the session ended".into()));
        match fake.next() {
            RhaiEvent::Done { stopped: false, error: Some(error), line } => {
                assert!(error.contains("the session ended"), "{error}");
                assert_eq!(line, 2);
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn a_cancelled_password_stops_the_script_and_a_cancelled_question_answers_nothing() {
        let fake = Fake::start("let a = prompt(\"Name?\");\nstatus(`[${a}]`);\npassword(\"Password\");", &[]);
        let (id, _, call, args) = fake.call();
        assert_eq!((call.as_str(), args["secret"].clone()), ("ask", json!(false)));
        fake.answer(id, Ok(Value::Null));
        assert_eq!(fake.next(), RhaiEvent::Status { line: 2, text: "[]".into() });
        let (id, _, _, _) = fake.call();
        fake.answer(id, Ok(Value::Null));
        assert!(matches!(fake.next(), RhaiEvent::Done { error: Some(e), line: 3, .. } if e.contains("cancelled the password")));
    }

    #[test]
    fn a_page_that_has_gone_stops_the_script() {
        let runs = RhaiRuns::default();
        let (tx, events) = channel();
        let tx = Mutex::new(tx);
        // the page takes the first event (Done), then has gone for everything else
        runs.start("send(\"x\");".into(), BTreeMap::new(), HashMap::new(), move |e| matches!(e, RhaiEvent::Done { .. }) && tx.lock().unwrap().send(e).is_ok(), |_| {});
        assert!(matches!(events.recv_timeout(Duration::from_secs(5)).unwrap(), RhaiEvent::Done { error: Some(e), .. } if e.contains("the tab closed")));
    }

    #[test]
    fn a_wrong_wait_for_option_is_named() {
        let fake = Fake::start("wait_for(\"#\", #{ timout: 5 });", &[]);
        assert!(matches!(fake.next(), RhaiEvent::Done { error: Some(e), line: 1, .. } if e.contains("no option \"timout\"")));
    }

    #[test]
    fn check_names_a_syntax_errors_line_without_running_anything() {
        assert_eq!(check("send(\"a\");\nlet = 5;").len(), 1);
        assert_eq!(check("send(\"a\");\nlet = 5;")[0].line, 2);
        assert!(check("send(\"a\"); wait_for(\"#\");").is_empty());
    }

    #[test]
    fn env_gives_environment_variables_in_any_case_and_names_one_with_no_value() {
        let fake = Fake::start_with_env(
            "send_line(env.ad_user);\nsend_line(env[\"AD_User\"]);\nstatus(`${env.has(\"jump\")} ${env.has(\"ad_user\")}`);\nsend_line(env.jump);",
            &[],
            &[("ad_user", "jdoe")],
        );
        for _ in 0..2 {
            let (id, _, _, args) = fake.call();
            assert_eq!(args["text"], "jdoe");
            fake.answer(id, Ok(Value::Null));
        }
        assert_eq!(fake.next(), RhaiEvent::Status { line: 3, text: "false true".into() });
        match fake.next() {
            RhaiEvent::Done { error: Some(error), line: 4, .. } => {
                assert!(error.contains("the Environment Variable jump has no value: define it in Environment Variables"), "{error}");
            }
            other => panic!("expected the missing variable named, got {other:?}"),
        }
    }

    /// Every ```rhai block in docs/RHAI.md is a whole script Check script passes, so the guide's examples stay true
    /// as the functions change
    #[test]
    fn the_guides_examples_check_clean() {
        // lines end in \r\n where Git checks the guide out that way on Windows (core.autocrlf)
        let guide = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/RHAI.md")).unwrap().replace("\r\n", "\n");
        let blocks: Vec<&str> = guide.split("```rhai\n").skip(1).map(|rest| rest.split("```").next().unwrap()).collect();
        assert!(blocks.len() >= 8, "found {} blocks", blocks.len());
        for block in blocks {
            let issues = check(block);
            assert!(issues.is_empty(), "{issues:?} in:\n{block}");
        }
    }

    #[test]
    fn a_login_scripts_rhai_marker_is_read_as_a_comment_keeping_the_lines() {
        assert!(check("#!/bin/rhai\nsend_line(\"x\");").is_empty());
        // a mistake on line 3 is still said to be on line 3
        assert_eq!(check("#!/bin/rhai\nlet a = 1;\nlet = 2;")[0].line, 3);
        let fake = Fake::start("#!/bin/rhai\nstatus(\"ran\");", &[]);
        assert_eq!(fake.next(), RhaiEvent::Status { line: 2, text: "ran".into() });
    }

    #[test]
    fn functions_see_session_and_env() {
        let fake = Fake::start_with_env("fn login() { send_line(`${env.ad_user}@${session.host_address}`); }\nlogin();", &[("host_address", "10.0.0.1")], &[("ad_user", "jdoe")]);
        let (id, _, _, args) = fake.call();
        assert_eq!(args["text"], "jdoe@10.0.0.1");
        fake.answer(id, Ok(Value::Null));
        assert!(matches!(fake.next(), RhaiEvent::Done { error: None, .. }));
    }

    #[test]
    fn a_session_value_the_tab_hasnt_stops_the_script_rather_than_typing_nothing() {
        let fake = Fake::start("send_line(`ssh ${session.host_address}`);", &[]);
        assert!(matches!(fake.next(), RhaiEvent::Done { error: Some(e), line: 1, .. } if e.contains("this tab has no session.host_address")), "nothing typed");
        let fake = Fake::start("status(session.hostname);", &[("host_address", "10.0.0.1")]);
        assert!(matches!(fake.next(), RhaiEvent::Done { error: Some(e), .. } if e.contains("session has no hostname: its values are host_address")));
        let fake = Fake::start("status(`${session.has(\"host_address\")} ${session.HOST_ADDRESS}`);", &[("host_address", "10.0.0.1")]);
        assert_eq!(fake.next(), RhaiEvent::Status { line: 1, text: "true 10.0.0.1".into() });
    }

    #[test]
    fn env_host_address_says_it_is_the_sessions() {
        let fake = Fake::start("send_line(env.host_address);", &[("host_address", "10.0.0.1")]);
        assert!(matches!(fake.next(), RhaiEvent::Done { error: Some(e), .. } if e.contains("env.host_address: host_address is the tab's session's, not an Environment Variable: write session.host_address")));
    }

    #[test]
    fn check_knows_env() {
        assert!(check("send_line(env.ad_user); if env.has(\"x\") { }\nfn f() { session.host_address + env.x }").is_empty());
    }

    #[test]
    fn throw_stops_the_script_with_its_own_message() {
        let fake = Fake::start("status(\"a\");\nthrow \"the switch isn't in config mode\";", &[]);
        let _ = fake.next();
        assert_eq!(fake.next(), RhaiEvent::Done { stopped: false, error: Some("the switch isn't in config mode".into()), line: 2 });
    }

    /// The examples in docs/rhai-examples are scripts Check script passes, so they don't rot as the functions change
    #[test]
    fn the_example_scripts_check_clean() {
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/rhai-examples");
        let mut checked = 0;
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "rhai") {
                let issues = check(&std::fs::read_to_string(&path).unwrap());
                assert!(issues.is_empty(), "{}: {issues:?}", path.display());
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn check_finds_a_misspelt_variable_and_knows_session() {
        let issues = check("let host = session.host_address;\nsend_line(hots);");
        assert_eq!(issues.len(), 1, "{issues:?}");
        assert_eq!(issues[0].line, 2);
        assert!(issues[0].message.contains("hots"), "{}", issues[0].message);
    }

    #[test]
    fn a_misspelt_variable_stops_the_script_before_it_types_anything() {
        let fake = Fake::start("send_line(\"configure terminal\");\nsend_line(hots);", &[]);
        assert!(matches!(fake.next(), RhaiEvent::Done { error: Some(e), line: 2, .. } if e.contains("hots")));
    }


}
