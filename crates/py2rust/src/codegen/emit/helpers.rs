// Helper function emission for generated Rust files.

use super::super::*;

/// Static helper body for `print(...)` calls.
const HELPER_PY_PRINT: &str = r#"
fn py_print<T: std::fmt::Display + ?Sized>(v: &T) {
    println!("{v}");
}
"#;

/// Static helper body for normalizing integer operands that may be borrowed.
const HELPER_PY_INT: &str = r#"
fn py_int(value: impl std::borrow::Borrow<i64>) -> i64 {
    *value.borrow()
}
"#;

/// Static helper body for Python `type(...)` name extraction.
const HELPER_PY_TYPE_NAME: &str = r#"
fn py_type_name<T: ?Sized>(value: &T) -> String {
    std::any::type_name_of_val(value).to_string()
}
"#;

/// Static helper body for iterator `next(...)` behavior.
const HELPER_PY_NEXT: &str = r#"
fn py_next<T>(value: Option<T>) -> Result<T, PyError> {
    value.ok_or_else(|| PyError::StopIteration(String::new().into()))
}
"#;

/// Static helper body for Python-style string indexing with negative indices.
const HELPER_PY_STR_GET: &str = r#"
fn py_str_get(s: &str, idx: i64) -> Result<String, PyError> {
    if idx >= 0 {
        return s
            .chars()
            .nth(idx as usize)
            .map(|ch| ch.to_string())
            .ok_or_else(|| PyError::IndexError("IndexError".into()));
    }
    // Negative indexing is resolved from the end in one iterator pass.
    let from_end = idx
        .checked_neg()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| PyError::IndexError("IndexError".into()))?;
    s.chars()
        .rev()
        .nth(from_end.saturating_sub(1))
        .map(|ch| ch.to_string())
        .ok_or_else(|| PyError::IndexError("IndexError".into()))
}
"#;

/// Static helper body for `os.remove(path)`.
const HELPER_PY_OS_REMOVE: &str = r#"
fn py_os_remove(path: &str) -> Result<(), PyError> {
    std::fs::remove_file(path).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.getcwd()`.
const HELPER_PY_OS_GETCWD: &str = r#"
fn py_os_getcwd() -> Result<String, PyError> {
    std::env::current_dir()
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.chdir(path)`.
const HELPER_PY_OS_CHDIR: &str = r#"
fn py_os_chdir(path: &str) -> Result<(), PyError> {
    std::env::set_current_dir(path).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.mkdir(path)`.
const HELPER_PY_OS_MKDIR: &str = r#"
fn py_os_mkdir(path: &str) -> Result<(), PyError> {
    std::fs::create_dir(path).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.listdir(path)`.
const HELPER_PY_OS_LISTDIR: &str = r#"
fn py_os_listdir(path: &str) -> Result<Arc<Mutex<Vec<String>>>, PyError> {
    let mut entries = Vec::new();
    let dir = std::fs::read_dir(path).map_err(|e| PyError::IOError(e.to_string().into()))?;
    for entry in dir {
        let entry = entry.map_err(|e| PyError::IOError(e.to_string().into()))?;
        entries.push(entry.file_name().to_string_lossy().to_string());
    }
    Ok(Arc::new(Mutex::new(entries)))
}
"#;

/// Static helper body for `os.rmdir(path)`.
const HELPER_PY_OS_RMDIR: &str = r#"
fn py_os_rmdir(path: &str) -> Result<(), PyError> {
    std::fs::remove_dir(path).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.rename(src, dst)`.
const HELPER_PY_OS_RENAME: &str = r#"
fn py_os_rename(src: &str, dst: &str) -> Result<(), PyError> {
    std::fs::rename(src, dst).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.replace(src, dst)`.
const HELPER_PY_OS_REPLACE: &str = r#"
fn py_os_replace(src: &str, dst: &str) -> Result<(), PyError> {
    std::fs::rename(src, dst).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.makedirs(path, exist_ok)`.
const HELPER_PY_OS_MAKEDIRS: &str = r#"
fn py_os_makedirs(path: &str, exist_ok: bool) -> Result<(), PyError> {
    if !exist_ok && std::path::Path::new(path).exists() {
        return Err(PyError::IOError(format!("File exists: {}", path).into()));
    }
    std::fs::create_dir_all(path).map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for `os.getenv(key, default)`.
const HELPER_PY_OS_GETENV: &str = r#"
fn py_os_getenv(key: &str, default: Option<String>) -> Option<String> {
    std::env::var(key).ok().or(default)
}
"#;

/// Static helper body for `os.environ`.
const HELPER_PY_OS_ENVIRON: &str = r#"
fn py_os_environ() -> Arc<Mutex<indexmap::IndexMap<String, String>>> {
    Arc::new(Mutex::new(
        std::env::vars().collect::<indexmap::IndexMap<_, _>>(),
    ))
}
"#;

/// Static helper body for `os.name`.
const HELPER_PY_OS_NAME: &str = r#"
fn py_os_name() -> String {
    if cfg!(windows) {
        "nt".to_string()
    } else {
        "posix".to_string()
    }
}
"#;

/// Static helper body for `os.path.join(parts...)`.
const HELPER_PY_OS_PATH_JOIN: &str = r#"
fn py_os_path_join(parts: &[&str]) -> String {
    let mut path = std::path::PathBuf::new();
    for part in parts {
        path.push(part);
    }
    path.to_string_lossy().to_string()
}
"#;

/// Static helper body for `os.path.exists(path)`.
const HELPER_PY_OS_PATH_EXISTS: &str = r#"
fn py_os_path_exists(path: &str) -> bool {
    std::path::Path::new(path).exists()
}
"#;

/// Static helper body for `os.path.isfile(path)`.
const HELPER_PY_OS_PATH_IS_FILE: &str = r#"
fn py_os_path_is_file(path: &str) -> bool {
    std::path::Path::new(path).is_file()
}
"#;

/// Static helper body for `os.path.isdir(path)`.
const HELPER_PY_OS_PATH_IS_DIR: &str = r#"
fn py_os_path_is_dir(path: &str) -> bool {
    std::path::Path::new(path).is_dir()
}
"#;

/// Static helper body for `os.path.basename(path)`.
const HELPER_PY_OS_PATH_BASENAME: &str = r#"
fn py_os_path_basename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}
"#;

/// Static helper body for `os.path.dirname(path)`.
const HELPER_PY_OS_PATH_DIRNAME: &str = r#"
fn py_os_path_dirname(path: &str) -> String {
    std::path::Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default()
}
"#;

/// Static helper body for `os.path.split(path)`.
const HELPER_PY_OS_PATH_SPLIT: &str = r#"
fn py_os_path_split(path: &str) -> (String, String) {
    let p = std::path::Path::new(path);
    let dir = p
        .parent()
        .map(|parent| parent.to_string_lossy().to_string())
        .unwrap_or_default();
    let base = p
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    (dir, base)
}
"#;

/// Static helper body for `os.path.abspath(path)`.
const HELPER_PY_OS_PATH_ABSPATH: &str = r#"
fn py_os_path_abspath(path: &str) -> Result<String, PyError> {
    let candidate = std::path::PathBuf::from(path);
    if candidate.is_absolute() {
        return Ok(candidate.to_string_lossy().to_string());
    }
    let cwd = std::env::current_dir().map_err(|e| PyError::IOError(e.to_string().into()))?;
    Ok(cwd.join(candidate).to_string_lossy().to_string())
}
"#;

/// Static helper body for `sys.argv`.
const HELPER_PY_SYS_ARGV: &str = r#"
fn py_sys_argv() -> Arc<Mutex<Vec<String>>> {
    Arc::new(Mutex::new(std::env::args().collect()))
}
"#;

/// Static helper body for `sys.intern(string)`.
const HELPER_PY_SYS_INTERN: &str = r#"
fn py_sys_intern(value: &str) -> String {
    value.to_string()
}
"#;

/// Static helper body for lightweight `subprocess.CompletedProcess` storage.
const HELPER_PY_SUBPROCESS_COMPLETED_PROCESS: &str = r#"
#[allow(non_camel_case_types)]
#[derive(Clone)]
struct __stdlib_subprocess_completed_process {
    args: Arc<Mutex<Vec<String>>>,
    returncode: i64,
    stdout: Option<String>,
    stderr: Option<String>,
}
"#;

/// Static helper body for `subprocess.run(args, capture_output=False, check=False)`.
const HELPER_PY_SUBPROCESS_RUN: &str = r#"
trait PySubprocessArgs {
    fn to_subprocess_args(&self) -> Vec<String>;
}

impl PySubprocessArgs for Vec<String> {
    fn to_subprocess_args(&self) -> Vec<String> {
        self.clone()
    }
}

impl PySubprocessArgs for Arc<Mutex<Vec<String>>> {
    fn to_subprocess_args(&self) -> Vec<String> {
        self.lock().expect("subprocess args list mutex poisoned").clone()
    }
}

impl PySubprocessArgs for Rc<RefCell<Vec<String>>> {
    fn to_subprocess_args(&self) -> Vec<String> {
        self.borrow().clone()
    }
}

fn py_subprocess_run(
    args: &impl PySubprocessArgs,
    capture_output: bool,
    check: bool,
) -> Result<__stdlib_subprocess_completed_process, PyError> {
    let args = args.to_subprocess_args();
    if args.is_empty() {
        return Err(PyError::ValueError(
            "subprocess.run() args must contain at least one command element".into(),
        ));
    }

    let mut command = std::process::Command::new(&args[0]);
    if args.len() > 1 {
        command.args(&args[1..]);
    }

    if capture_output {
        let output = command
            .output()
            .map_err(|e| PyError::IOError(e.to_string().into()))?;
        let returncode = output.status.code().unwrap_or(-1);
        if check && returncode != 0 {
            return Err(PyError::RuntimeError(
                format!("subprocess.run() check failed with returncode {}", returncode).into(),
            ));
        }
        return Ok(__stdlib_subprocess_completed_process {
            args: Arc::new(Mutex::new(args)),
            returncode: returncode as i64,
            stdout: Some(String::from_utf8_lossy(&output.stdout).to_string()),
            stderr: Some(String::from_utf8_lossy(&output.stderr).to_string()),
        });
    }

    let status = command
        .status()
        .map_err(|e| PyError::IOError(e.to_string().into()))?;
    let returncode = status.code().unwrap_or(-1);
    if check && returncode != 0 {
        return Err(PyError::RuntimeError(
            format!("subprocess.run() check failed with returncode {}", returncode).into(),
        ));
    }
    Ok(__stdlib_subprocess_completed_process {
        args: Arc::new(Mutex::new(args)),
        returncode: returncode as i64,
        stdout: None,
        stderr: None,
    })
}
"#;

/// Static helper body for lightweight `urllib.parse.ParseResult` storage.
const HELPER_PY_URLLIB_PARSE_RESULT_STRUCT: &str = r#"
#[allow(non_camel_case_types)]
#[derive(Clone)]
struct __py_urllib_parse_result {
    scheme: String,
    netloc: String,
    path: String,
    query: String,
    fragment: String,
}
"#;

/// Static helper body for lightweight `urllib.request` response storage.
const HELPER_PY_URLLIB_RESPONSE_STRUCT: &str = r#"
#[allow(non_camel_case_types)]
#[derive(Clone)]
struct __py_urllib_response {
    status: i64,
    url: String,
    headers: Arc<Mutex<indexmap::IndexMap<String, String>>>,
    body: String,
}
"#;

/// Static helper body for `urllib.parse.unquote(text)`.
const HELPER_PY_URLLIB_UNQUOTE: &str = r#"
fn py_urllib_hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn py_urllib_unquote(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut idx = 0usize;
    while idx < bytes.len() {
        let byte = bytes[idx];
        if byte == b'%' && idx + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (
                py_urllib_hex_value(bytes[idx + 1]),
                py_urllib_hex_value(bytes[idx + 2]),
            ) {
                out.push((hi << 4) | lo);
                idx += 3;
                continue;
            }
        }
        if byte == b'+' {
            out.push(b' ');
        } else {
            out.push(byte);
        }
        idx += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
"#;

/// Static helper body for `urllib.parse.quote(text, safe)`.
const HELPER_PY_URLLIB_QUOTE: &str = r#"
fn py_urllib_quote(text: &str, safe: &str) -> String {
    let safe_bytes = safe.as_bytes();
    let mut out = String::new();
    for byte in text.as_bytes() {
        let b = *byte;
        let is_unreserved = b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved || safe_bytes.contains(&b) {
            out.push(char::from(b));
            continue;
        }
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        out.push('%');
        out.push(char::from(HEX[(b >> 4) as usize]));
        out.push(char::from(HEX[(b & 0x0F) as usize]));
    }
    out
}
"#;

/// Static helper body for `urllib.parse.urlencode(params)`.
const HELPER_PY_URLLIB_URLENCODE: &str = r#"
fn py_urllib_urlencode(params: &Arc<Mutex<indexmap::IndexMap<String, String>>>) -> String {
    let guard = params.lock().expect("urllib urlencode dict mutex poisoned");
    let mut pairs: Vec<String> = Vec::new();
    for (key, value) in guard.iter() {
        pairs.push(format!(
            "{}={}",
            py_urllib_quote(key, ""),
            py_urllib_quote(value, "")
        ));
    }
    pairs.join("&")
}
"#;

/// Static helper body for `urllib.parse.parse_qs(query)`.
const HELPER_PY_URLLIB_PARSE_QS: &str = r#"
fn py_urllib_parse_qs(
    query: &str,
) -> Arc<Mutex<indexmap::IndexMap<String, Arc<Mutex<Vec<String>>>>>> {
    let mut out: indexmap::IndexMap<String, Arc<Mutex<Vec<String>>>> = indexmap::IndexMap::new();
    let raw = if let Some(rest) = query.strip_prefix('?') {
        rest
    } else {
        query
    };
    if raw.is_empty() {
        return Arc::new(Mutex::new(out));
    }
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut split = pair.splitn(2, '=');
        let key = py_urllib_unquote(split.next().unwrap_or_default());
        let value = py_urllib_unquote(split.next().unwrap_or_default());
        if let Some(values) = out.get(&key) {
            values
                .lock()
                .expect("urllib parse_qs value list mutex poisoned")
                .push(value);
            continue;
        }
        out.insert(key, Arc::new(Mutex::new(vec![value])));
    }
    Arc::new(Mutex::new(out))
}
"#;

/// Static helper body for `urllib.parse.urlparse(url)`.
const HELPER_PY_URLLIB_URLPARSE: &str = r#"
fn py_urllib_urlparse(url: &str) -> __py_urllib_parse_result {
    let mut rest = url;
    let mut scheme = String::new();
    if let Some(idx) = rest.find("://") {
        scheme = rest[..idx].to_string();
        rest = &rest[(idx + 3)..];
    }

    let mut fragment = String::new();
    if let Some(idx) = rest.find('#') {
        fragment = rest[(idx + 1)..].to_string();
        rest = &rest[..idx];
    }

    let mut query = String::new();
    if let Some(idx) = rest.find('?') {
        query = rest[(idx + 1)..].to_string();
        rest = &rest[..idx];
    }

    let (netloc, path) = if !scheme.is_empty() {
        if let Some(idx) = rest.find('/') {
            (rest[..idx].to_string(), rest[idx..].to_string())
        } else {
            (rest.to_string(), String::new())
        }
    } else if let Some(no_slashes) = rest.strip_prefix("//") {
        if let Some(idx) = no_slashes.find('/') {
            (no_slashes[..idx].to_string(), no_slashes[idx..].to_string())
        } else {
            (no_slashes.to_string(), String::new())
        }
    } else {
        (String::new(), rest.to_string())
    };

    __py_urllib_parse_result {
        scheme,
        netloc,
        path,
        query,
        fragment,
    }
}
"#;

/// Static helper body for `urllib.parse.ParseResult.geturl()`.
const HELPER_PY_URLLIB_PARSE_GETURL: &str = r#"
fn py_urllib_parse_geturl(value: &__py_urllib_parse_result) -> String {
    let mut out = String::new();
    if !value.scheme.is_empty() {
        out.push_str(&value.scheme);
        out.push_str("://");
    } else if !value.netloc.is_empty() {
        out.push_str("//");
    }
    out.push_str(&value.netloc);
    out.push_str(&value.path);
    if !value.query.is_empty() {
        out.push('?');
        out.push_str(&value.query);
    }
    if !value.fragment.is_empty() {
        out.push('#');
        out.push_str(&value.fragment);
    }
    out
}
"#;

/// Static helper body for `urllib.parse.urljoin(base, url)`.
const HELPER_PY_URLLIB_URLJOIN: &str = r#"
fn py_urllib_normalize_path(path: &str) -> String {
    let leading_slash = path.starts_with('/');
    let trailing_slash = path.ends_with('/') && path.len() > 1;
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            if !parts.is_empty() {
                let _ = parts.pop();
            }
            continue;
        }
        parts.push(part);
    }
    let mut out = String::new();
    if leading_slash {
        out.push('/');
    }
    out.push_str(&parts.join("/"));
    if trailing_slash && !out.ends_with('/') {
        out.push('/');
    }
    if out.is_empty() && leading_slash {
        out.push('/');
    }
    out
}

fn py_urllib_urljoin(base: &str, url: &str) -> String {
    if url.contains("://") {
        return url.to_string();
    }
    if url.starts_with("//") {
        let parsed = py_urllib_urlparse(base);
        if parsed.scheme.is_empty() {
            return format!("//{}", &url[2..]);
        }
        return format!("{}:{}", parsed.scheme, url);
    }

    let mut base_parsed = py_urllib_urlparse(base);
    if url.is_empty() {
        return py_urllib_parse_geturl(&base_parsed);
    }
    if let Some(fragment) = url.strip_prefix('#') {
        base_parsed.fragment = fragment.to_string();
        return py_urllib_parse_geturl(&base_parsed);
    }
    if let Some(query) = url.strip_prefix('?') {
        base_parsed.query = query.to_string();
        base_parsed.fragment.clear();
        return py_urllib_parse_geturl(&base_parsed);
    }
    if url.starts_with('/') {
        base_parsed.path = py_urllib_normalize_path(url);
        base_parsed.query.clear();
        base_parsed.fragment.clear();
        return py_urllib_parse_geturl(&base_parsed);
    }

    let mut base_dir = base_parsed.path.clone();
    if let Some(idx) = base_dir.rfind('/') {
        base_dir.truncate(idx + 1);
    } else {
        base_dir.clear();
        if !base_parsed.netloc.is_empty() {
            base_dir.push('/');
        }
    }
    let joined = format!("{}{}", base_dir, url);
    base_parsed.path = py_urllib_normalize_path(&joined);
    base_parsed.query.clear();
    base_parsed.fragment.clear();
    py_urllib_parse_geturl(&base_parsed)
}
"#;

/// Static helper body for `urllib.request.urlopen`.
const HELPER_PY_URLLIB_URLOPEN: &str = r#"
fn py_urllib_urlopen(
    url: &str,
    data: Option<Vec<i64>>,
    timeout: Option<f64>,
) -> Result<__py_urllib_response, PyError> {
    if let Some(value) = timeout {
        if !value.is_finite() || value < 0.0 {
            return Err(PyError::ValueError(
                "urllib.request.urlopen() timeout must be a non-negative finite float".into(),
            ));
        }
    }

    if let Some(path) = url.strip_prefix("file://") {
        if data.is_some() {
            return Err(PyError::ValueError(
                "urllib.request.urlopen() data is not supported for file:// urls".into(),
            ));
        }
        let decoded_path = py_urllib_unquote(path);
        let body = std::fs::read_to_string(&decoded_path)
            .map_err(|e| PyError::IOError(e.to_string().into()))?;
        let mut headers = indexmap::IndexMap::new();
        headers.insert("content-type".to_string(), "text/plain".to_string());
        headers.insert("content-length".to_string(), body.len().to_string());
        return Ok(__py_urllib_response {
            status: 200,
            url: url.to_string(),
            headers: Arc::new(Mutex::new(headers)),
            body,
        });
    }

    if let Some(payload) = url.strip_prefix("data:text/plain,") {
        if data.is_some() {
            return Err(PyError::ValueError(
                "urllib.request.urlopen() data argument is not supported for data: urls".into(),
            ));
        }
        let body = py_urllib_unquote(payload);
        let mut headers = indexmap::IndexMap::new();
        headers.insert("content-type".to_string(), "text/plain".to_string());
        headers.insert("content-length".to_string(), body.len().to_string());
        return Ok(__py_urllib_response {
            status: 200,
            url: url.to_string(),
            headers: Arc::new(Mutex::new(headers)),
            body,
        });
    }

    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(PyError::ValueError(
            "urllib.request.urlopen() currently supports file://, data:text/plain,, http://, and https:// urls"
                .into(),
        ));
    }

    // Python bytes map to Vec<i64>; normalize to u8 for HTTP body sending.
    let request_body = if let Some(values) = data {
        let mut bytes = Vec::with_capacity(values.len());
        for value in values {
            let byte = u8::try_from(value).map_err(|_| {
                PyError::ValueError(
                    "urllib.request.urlopen() data bytes must be in range(0, 256)".into(),
                )
            })?;
            bytes.push(byte);
        }
        Some(bytes)
    } else {
        None
    };

    // ureq gives us a lightweight blocking client without requiring an async runtime.
    let mut builder = ureq::AgentBuilder::new();
    if let Some(value) = timeout {
        builder = builder.timeout(std::time::Duration::from_secs_f64(value));
    }
    let agent = builder.build();

    // Match urllib behavior: data present => POST, otherwise GET.
    let response_result = if let Some(body) = request_body {
        agent.post(url).send_bytes(&body)
    } else {
        agent.get(url).call()
    };

    let response = match response_result {
        Ok(response) => response,
        // Keep HTTP status responses as normal response objects for lightweight
        // compatibility with current py2rust runtime expectations.
        Err(ureq::Error::Status(_status, response)) => response,
        Err(ureq::Error::Transport(err)) => {
            return Err(PyError::IOError(
                format!("urllib.request.urlopen() transport error: {err}").into(),
            ));
        }
    };

    let status = i64::from(response.status());
    let response_url = response.get_url().to_string();
    let mut headers = indexmap::IndexMap::new();
    for name in response.headers_names() {
        if let Some(value) = response.header(&name) {
            // Python-style header lookup is case-insensitive; store lowercase keys.
            headers.insert(name.to_ascii_lowercase(), value.to_string());
        }
    }
    let body = response.into_string().map_err(|err| {
        PyError::IOError(format!("urllib.request.urlopen() failed to read body: {err}").into())
    })?;

    Ok(__py_urllib_response {
        status,
        url: response_url,
        headers: Arc::new(Mutex::new(headers)),
        body,
    })
}
"#;

/// Static helper body for `urllib.request response.read()`.
const HELPER_PY_URLLIB_RESPONSE_READ: &str = r#"
fn py_urllib_response_read(value: &__py_urllib_response) -> String {
    value.body.clone()
}
"#;

/// Static helper body for `urllib.request response.getcode()`.
const HELPER_PY_URLLIB_RESPONSE_GETCODE: &str = r#"
fn py_urllib_response_getcode(value: &__py_urllib_response) -> i64 {
    value.status
}
"#;

/// Static helper body for `urllib.request response.geturl()`.
const HELPER_PY_URLLIB_RESPONSE_GETURL: &str = r#"
fn py_urllib_response_geturl(value: &__py_urllib_response) -> String {
    value.url.clone()
}
"#;

/// Static helper body for `math.factorial(n)`.
const HELPER_PY_MATH_FACTORIAL: &str = r#"
fn py_math_factorial(value: i64) -> Result<i64, PyError> {
    if value < 0 {
        return Err(PyError::ValueError(
            "factorial() not defined for negative values".into(),
        ));
    }
    let mut acc: i128 = 1;
    let mut n: i128 = 2;
    let end = value as i128;
    while n <= end {
        acc = acc
            .checked_mul(n)
            .ok_or_else(|| PyError::OverflowError("factorial() result too large".into()))?;
        n += 1;
    }
    i64::try_from(acc).map_err(|_| PyError::OverflowError("factorial() result too large".into()))
}
"#;

/// Static helper body for `math.gcd(a, b)`.
const HELPER_PY_MATH_GCD: &str = r#"
fn py_math_gcd(a: i64, b: i64) -> i64 {
    let mut x = (a as i128).abs();
    let mut y = (b as i128).abs();
    while y != 0 {
        let r = x % y;
        x = y;
        y = r;
    }
    x as i64
}
"#;

/// Static helper body for `math.lcm(a, b)`.
const HELPER_PY_MATH_LCM: &str = r#"
fn py_math_lcm(a: i64, b: i64) -> Result<i64, PyError> {
    if a == 0 || b == 0 {
        return Ok(0);
    }
    // Keep this helper self-contained so it can be emitted without py_math_gcd.
    let mut x = (a as i128).abs();
    let mut y = (b as i128).abs();
    while y != 0 {
        let r = x % y;
        x = y;
        y = r;
    }
    let gcd = x;
    let lcm = ((a as i128) / gcd)
        .checked_mul(b as i128)
        .ok_or_else(|| PyError::OverflowError("lcm() result too large".into()))?
        .abs();
    i64::try_from(lcm).map_err(|_| PyError::OverflowError("lcm() result too large".into()))
}
"#;

/// Static helper body for `math.comb(n, k)`.
const HELPER_PY_MATH_COMB: &str = r#"
fn py_math_comb(n: i64, k: i64) -> Result<i64, PyError> {
    if n < 0 || k < 0 {
        return Err(PyError::ValueError(
            "comb() not defined for negative values".into(),
        ));
    }
    if k > n {
        return Ok(0);
    }
    let n128 = n as i128;
    let mut k128 = k as i128;
    let alt = n128 - k128;
    if alt < k128 {
        k128 = alt;
    }
    let mut result: i128 = 1;
    let mut i: i128 = 0;
    while i < k128 {
        let numerator = n128 - i;
        let denominator = i + 1;
        result = result
            .checked_mul(numerator)
            .ok_or_else(|| PyError::OverflowError("comb() result too large".into()))?
            / denominator;
        i += 1;
    }
    i64::try_from(result).map_err(|_| PyError::OverflowError("comb() result too large".into()))
}
"#;

/// Static helper body for `math.perm(n, k)`.
const HELPER_PY_MATH_PERM: &str = r#"
fn py_math_perm(n: i64, k: i64) -> Result<i64, PyError> {
    if n < 0 || k < 0 {
        return Err(PyError::ValueError(
            "perm() not defined for negative values".into(),
        ));
    }
    if k > n {
        return Ok(0);
    }
    let mut result: i128 = 1;
    let mut factor = (n - k + 1) as i128;
    let end = n as i128;
    while factor <= end {
        result = result
            .checked_mul(factor)
            .ok_or_else(|| PyError::OverflowError("perm() result too large".into()))?;
        factor += 1;
    }
    i64::try_from(result).map_err(|_| PyError::OverflowError("perm() result too large".into()))
}
"#;

/// Static helper body for `time.monotonic()`.
const HELPER_PY_TIME_MONOTONIC: &str = r#"
fn py_time_monotonic() -> f64 {
    static __PY_TIME_MONOTONIC_START: OnceLock<std::time::Instant> = OnceLock::new();
    __PY_TIME_MONOTONIC_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}
"#;

/// Static helper body for `time.monotonic_ns()`.
const HELPER_PY_TIME_MONOTONIC_NS: &str = r#"
fn py_time_monotonic_ns() -> i64 {
    static __PY_TIME_MONOTONIC_NS_START: OnceLock<std::time::Instant> = OnceLock::new();
    __PY_TIME_MONOTONIC_NS_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as i64
}
"#;

/// Static helper body for `time.perf_counter()`.
const HELPER_PY_TIME_PERF_COUNTER: &str = r#"
fn py_time_perf_counter() -> f64 {
    static __PY_TIME_PERF_COUNTER_START: OnceLock<std::time::Instant> = OnceLock::new();
    __PY_TIME_PERF_COUNTER_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}
"#;

/// Static helper body for `time.perf_counter_ns()`.
const HELPER_PY_TIME_PERF_COUNTER_NS: &str = r#"
fn py_time_perf_counter_ns() -> i64 {
    static __PY_TIME_PERF_COUNTER_NS_START: OnceLock<std::time::Instant> = OnceLock::new();
    __PY_TIME_PERF_COUNTER_NS_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as i64
}
"#;

/// Static helper body for `time.process_time()`.
const HELPER_PY_TIME_PROCESS_TIME: &str = r#"
fn py_time_process_time() -> f64 {
    static __PY_TIME_PROCESS_TIME_START: OnceLock<std::time::Instant> = OnceLock::new();
    __PY_TIME_PROCESS_TIME_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
}
"#;

/// Static helper body for `time.process_time_ns()`.
const HELPER_PY_TIME_PROCESS_TIME_NS: &str = r#"
fn py_time_process_time_ns() -> i64 {
    static __PY_TIME_PROCESS_TIME_NS_START: OnceLock<std::time::Instant> = OnceLock::new();
    __PY_TIME_PROCESS_TIME_NS_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_nanos() as i64
}
"#;

/// Static helper body for `time.sleep(seconds)`.
const HELPER_PY_TIME_SLEEP: &str = r#"
fn py_time_sleep(seconds: f64) {
    if !seconds.is_finite() || seconds <= 0.0 {
        return;
    }
    std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
}
"#;

/// Static helper body for shared lightweight `time` tuple/date primitives.
const HELPER_PY_TIME_CORE: &str = r#"
type __py_time_tuple = (i64, i64, i64, i64, i64, i64, i64, i64, i64);

fn py_time_is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn py_time_days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 => 31,
        2 => if py_time_is_leap(year) { 29 } else { 28 },
        3 => 31,
        4 => 30,
        5 => 31,
        6 => 30,
        7 => 31,
        8 => 31,
        9 => 30,
        10 => 31,
        11 => 30,
        12 => 31,
        _ => panic!("invalid month in time tuple: {}", month),
    }
}

fn py_time_day_of_year(year: i64, month: i64, mday: i64) -> i64 {
    let mut yday = 0;
    let mut m = 1;
    while m < month {
        yday += py_time_days_in_month(year, m);
        m += 1;
    }
    yday + mday
}

fn py_time_civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let mday = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, mday)
}

fn py_time_days_from_civil(year: i64, month: i64, mday: i64) -> i64 {
    let y = year - if month <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + mday - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn py_time_split_epoch(seconds: f64) -> __py_time_tuple {
    let whole = seconds.floor() as i64;
    let day_seconds = whole.rem_euclid(86_400);
    let days = whole.div_euclid(86_400);
    let hour = day_seconds / 3600;
    let minute = (day_seconds % 3600) / 60;
    let second = day_seconds % 60;
    let (year, month, mday) = py_time_civil_from_days(days);
    // Python tm_wday is Monday=0 .. Sunday=6.
    let wday = (days + 3).rem_euclid(7);
    let yday = py_time_day_of_year(year, month, mday);
    (year, month, mday, hour, minute, second, wday, yday, 0)
}

fn py_time_now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::from_secs(0))
        .as_secs_f64()
}

fn py_time_parse_fixed_digits(text: &[u8], idx: &mut usize, width: usize, field: &str) -> i64 {
    if *idx + width > text.len() {
        panic!("time.strptime(): unexpected end while parsing {}", field);
    }
    for pos in *idx..(*idx + width) {
        if !text[pos].is_ascii_digit() {
            panic!("time.strptime(): expected digits for {}", field);
        }
    }
    let slice = std::str::from_utf8(&text[*idx..(*idx + width)])
        .unwrap_or_else(|_| panic!("time.strptime(): invalid utf-8 in {}", field));
    *idx += width;
    slice
        .parse::<i64>()
        .unwrap_or_else(|_| panic!("time.strptime(): invalid number for {}", field))
}
"#;

/// Static helper body for `time.localtime([secs])`.
const HELPER_PY_TIME_LOCALTIME: &str = r#"
fn py_time_local_offset_seconds(year: i32, month: u32, mday: u32) -> i32 {
    // Sample noon instead of midnight to avoid ambiguous/non-existent local midnight
    // around DST transitions.
    let local_result = chrono::TimeZone::with_ymd_and_hms(&chrono::Local, year, month, mday, 12, 0, 0);
    let dt = local_result
        .single()
        .or_else(|| local_result.earliest())
        .or_else(|| local_result.latest())
        .unwrap_or_else(|| panic!("time.localtime(): invalid local date {}-{:02}-{:02}", year, month, mday));
    chrono::Offset::fix(dt.offset()).local_minus_utc()
}

fn py_time_is_dst_local(dt: &chrono::DateTime<chrono::Local>) -> i64 {
    let year = chrono::Datelike::year(dt);
    let jan_offset = py_time_local_offset_seconds(year, 1, 1);
    let jul_offset = py_time_local_offset_seconds(year, 7, 1);
    if jan_offset == jul_offset {
        return 0;
    }
    let current_offset = chrono::Offset::fix(dt.offset()).local_minus_utc();
    let standard_offset = std::cmp::min(jan_offset, jul_offset);
    if current_offset > standard_offset {
        1
    } else {
        0
    }
}

fn py_time_localtime(seconds: Option<f64>) -> __py_time_tuple {
    let seconds = seconds.unwrap_or_else(py_time_now_seconds);
    let whole = seconds.floor() as i64;
    let utc = chrono::DateTime::<chrono::Utc>::from_timestamp(whole, 0)
        .unwrap_or_else(|| panic!("time.localtime(): timestamp out of range"));
    let local = utc.with_timezone(&chrono::Local);
    (
        i64::from(chrono::Datelike::year(&local)),
        i64::from(chrono::Datelike::month(&local)),
        i64::from(chrono::Datelike::day(&local)),
        i64::from(chrono::Timelike::hour(&local)),
        i64::from(chrono::Timelike::minute(&local)),
        i64::from(chrono::Timelike::second(&local)),
        i64::from(chrono::Datelike::weekday(&local).num_days_from_monday()),
        i64::from(chrono::Datelike::ordinal(&local)),
        py_time_is_dst_local(&local),
    )
}
"#;

/// Static helper body for `time.gmtime([secs])`.
const HELPER_PY_TIME_GMTIME: &str = r#"
fn py_time_gmtime(seconds: Option<f64>) -> __py_time_tuple {
    py_time_split_epoch(seconds.unwrap_or_else(py_time_now_seconds))
}
"#;

/// Static helper body for `time.strftime(format, tuple)`.
const HELPER_PY_TIME_STRFTIME: &str = r#"
fn py_time_strftime(format: &str, tm: &__py_time_tuple) -> String {
    let (year, month, mday, hour, minute, second, wday, yday, _isdst) = *tm;
    let weekday_short = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let weekday_long = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let month_short = [
        "Jan",
        "Feb",
        "Mar",
        "Apr",
        "May",
        "Jun",
        "Jul",
        "Aug",
        "Sep",
        "Oct",
        "Nov",
        "Dec",
    ];
    let month_long = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];

    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        let directive = chars
            .next()
            .unwrap_or_else(|| panic!("time.strftime(): trailing '%' in format"));
        match directive {
            '%' => out.push('%'),
            'Y' => out.push_str(&format!("{:04}", year)),
            'm' => out.push_str(&format!("{:02}", month)),
            'd' => out.push_str(&format!("{:02}", mday)),
            'H' => out.push_str(&format!("{:02}", hour)),
            'M' => out.push_str(&format!("{:02}", minute)),
            'S' => out.push_str(&format!("{:02}", second)),
            'j' => out.push_str(&format!("{:03}", yday)),
            // Python %w is Sunday=0..Saturday=6.
            'w' => out.push_str(&((wday + 1).rem_euclid(7)).to_string()),
            'a' => {
                let idx = usize::try_from(wday)
                    .unwrap_or_else(|_| panic!("time.strftime(): invalid weekday {}", wday));
                out.push_str(weekday_short.get(idx).unwrap_or_else(|| {
                    panic!("time.strftime(): invalid weekday {}", wday)
                }));
            }
            'A' => {
                let idx = usize::try_from(wday)
                    .unwrap_or_else(|_| panic!("time.strftime(): invalid weekday {}", wday));
                out.push_str(
                    weekday_long
                        .get(idx)
                        .unwrap_or_else(|| panic!("time.strftime(): invalid weekday {}", wday)),
                );
            }
            'b' => {
                let idx = usize::try_from(month - 1)
                    .unwrap_or_else(|_| panic!("time.strftime(): invalid month {}", month));
                out.push_str(
                    month_short
                        .get(idx)
                        .unwrap_or_else(|| panic!("time.strftime(): invalid month {}", month)),
                );
            }
            'B' => {
                let idx = usize::try_from(month - 1)
                    .unwrap_or_else(|_| panic!("time.strftime(): invalid month {}", month));
                out.push_str(
                    month_long
                        .get(idx)
                        .unwrap_or_else(|| panic!("time.strftime(): invalid month {}", month)),
                );
            }
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    out
}
"#;

/// Static helper body for `time.strptime(string, format)`.
const HELPER_PY_TIME_STRPTIME: &str = r#"
fn py_time_strptime(text: &str, format: &str) -> __py_time_tuple {
    let fmt = format.as_bytes();
    let txt = text.as_bytes();
    let mut fi: usize = 0;
    let mut ti: usize = 0;

    let mut year: i64 = 1900;
    let mut month: i64 = 1;
    let mut mday: i64 = 1;
    let mut hour: i64 = 0;
    let mut minute: i64 = 0;
    let mut second: i64 = 0;
    let mut wday: i64 = -1;
    let mut yday: i64 = -1;

    let mut has_month = false;
    let mut has_mday = false;
    let mut has_wday = false;
    let mut has_yday = false;

    while fi < fmt.len() {
        if fmt[fi] != b'%' {
            if ti >= txt.len() || txt[ti] != fmt[fi] {
                panic!("time.strptime(): literal mismatch");
            }
            fi += 1;
            ti += 1;
            continue;
        }
        fi += 1;
        if fi >= fmt.len() {
            panic!("time.strptime(): trailing '%' in format");
        }
        match fmt[fi] as char {
            '%' => {
                if ti >= txt.len() || txt[ti] != b'%' {
                    panic!("time.strptime(): expected '%' in input");
                }
                ti += 1;
            }
            'Y' => year = py_time_parse_fixed_digits(txt, &mut ti, 4, "year"),
            'm' => {
                month = py_time_parse_fixed_digits(txt, &mut ti, 2, "month");
                has_month = true;
            }
            'd' => {
                mday = py_time_parse_fixed_digits(txt, &mut ti, 2, "day");
                has_mday = true;
            }
            'H' => hour = py_time_parse_fixed_digits(txt, &mut ti, 2, "hour"),
            'M' => minute = py_time_parse_fixed_digits(txt, &mut ti, 2, "minute"),
            'S' => second = py_time_parse_fixed_digits(txt, &mut ti, 2, "second"),
            'j' => {
                yday = py_time_parse_fixed_digits(txt, &mut ti, 3, "day of year");
                has_yday = true;
            }
            'w' => {
                // %w is Sunday=0..Saturday=6; convert to Monday=0..Sunday=6.
                let sunday_wday = py_time_parse_fixed_digits(txt, &mut ti, 1, "weekday");
                wday = (sunday_wday + 6).rem_euclid(7);
                has_wday = true;
            }
            other => panic!("time.strptime(): unsupported format directive %{}", other),
        }
        fi += 1;
    }

    if ti != txt.len() {
        panic!("time.strptime(): trailing input");
    }

    if has_yday && !(has_month || has_mday) {
        let mut rem = yday;
        month = 1;
        while rem > py_time_days_in_month(year, month) {
            rem -= py_time_days_in_month(year, month);
            month += 1;
        }
        mday = rem;
    }

    if !has_yday {
        yday = py_time_day_of_year(year, month, mday);
    }
    if !has_wday {
        let days = py_time_days_from_civil(year, month, mday);
        wday = (days + 3).rem_euclid(7);
    }

    (year, month, mday, hour, minute, second, wday, yday, -1)
}
"#;

/// Static helper body for lightweight `re.Match` storage.
const HELPER_PY_RE_MATCH_STRUCT: &str = r#"
#[allow(non_camel_case_types)]
#[derive(Clone)]
struct __py_re_match {
    groups: Vec<Option<String>>,
    start: i64,
    end: i64,
}
"#;

/// Static helper body for shared lightweight `re` matching primitives.
const HELPER_PY_RE_CORE: &str = r#"
fn py_re_char_idx(text: &str, byte_idx: usize) -> i64 {
    text[..byte_idx].chars().count() as i64
}

fn py_re_capture_group_strings(captures: &regex_lite::Captures<'_>) -> Vec<Option<String>> {
    let mut groups: Vec<Option<String>> = Vec::new();
    for idx in 0..captures.len() {
        groups.push(captures.get(idx).map(|m| m.as_str().to_string()));
    }
    groups
}

fn py_re_build_match(text: &str, captures: regex_lite::Captures<'_>) -> __py_re_match {
    let full = captures
        .get(0)
        .unwrap_or_else(|| panic!("regex-lite returned captures without group 0"));
    __py_re_match {
        groups: py_re_capture_group_strings(&captures),
        start: py_re_char_idx(text, full.start()),
        end: py_re_char_idx(text, full.end()),
    }
}
"#;

/// Static helper body for `re.search(pattern, string)`.
const HELPER_PY_RE_SEARCH: &str = r#"
fn py_re_search(pattern: &str, text: &str) -> __py_re_match {
    let regex = regex_lite::Regex::new(pattern)
        .unwrap_or_else(|err| panic!("re.search(): invalid pattern '{}': {}", pattern, err));
    let captures = regex
        .captures(text)
        .unwrap_or_else(|| panic!("re.search(): pattern did not match"));
    py_re_build_match(text, captures)
}
"#;

/// Static helper body for `re.match(pattern, string)`.
const HELPER_PY_RE_MATCH_FN: &str = r#"
fn py_re_match(pattern: &str, text: &str) -> __py_re_match {
    let regex = regex_lite::Regex::new(pattern)
        .unwrap_or_else(|err| panic!("re.match(): invalid pattern '{}': {}", pattern, err));
    let captures = regex
        .captures(text)
        .and_then(|caps| match caps.get(0) {
            Some(m0) if m0.start() == 0 => Some(caps),
            _ => None,
        })
        .unwrap_or_else(|| panic!("re.match(): pattern did not match"));
    py_re_build_match(text, captures)
}
"#;

/// Static helper body for `re.sub(pattern, repl, string)`.
const HELPER_PY_RE_SUB: &str = r#"
fn py_re_sub(pattern: &str, repl: &str, text: &str) -> String {
    let regex = regex_lite::Regex::new(pattern)
        .unwrap_or_else(|err| panic!("re.sub(): invalid pattern '{}': {}", pattern, err));
    regex.replace_all(text, repl).to_string()
}
"#;

/// Static helper body for `re.Match.group(index)`.
const HELPER_PY_RE_GROUP: &str = r#"
fn py_re_group(value: &__py_re_match, index: i64) -> String {
    if index < 0 {
        panic!("re.Match.group() does not support negative group indexes");
    }
    let idx = usize::try_from(index)
        .unwrap_or_else(|_| panic!("re.Match.group(): invalid group index"));
    value
        .groups
        .get(idx)
        .and_then(|group| group.clone())
        .unwrap_or_else(|| panic!("re.Match.group(): missing capture group {}", index))
}
"#;

/// Static helper body for `re.Match.span()`.
const HELPER_PY_RE_SPAN: &str = r#"
fn py_re_span(value: &__py_re_match) -> (i64, i64) {
    (value.start, value.end)
}
"#;

/// Static helper body for shared lightweight `json` value support.
const HELPER_PY_JSON_CORE: &str = r#"
#[allow(non_camel_case_types)]
#[derive(Clone)]
enum __py_json_value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<__py_json_value>),
    Object(std::collections::HashMap<String, __py_json_value>),
}

trait PyToJsonValue {
    fn to_json_value(&self) -> Result<__py_json_value, PyError>;
}

impl PyToJsonValue for __py_json_value {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(self.clone())
    }
}

impl PyToJsonValue for () {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(__py_json_value::Null)
    }
}

impl PyToJsonValue for bool {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(__py_json_value::Bool(*self))
    }
}

impl PyToJsonValue for i64 {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(__py_json_value::Int(*self))
    }
}

impl PyToJsonValue for f64 {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(__py_json_value::Float(*self))
    }
}

impl PyToJsonValue for String {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(__py_json_value::Str(self.clone()))
    }
}

impl PyToJsonValue for &str {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        Ok(__py_json_value::Str((*self).to_string()))
    }
}

impl<T: PyToJsonValue> PyToJsonValue for Option<T> {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        match self {
            Some(value) => value.to_json_value(),
            None => Ok(__py_json_value::Null),
        }
    }
}

impl<T: PyToJsonValue> PyToJsonValue for Vec<T> {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        let mut items: Vec<__py_json_value> = Vec::new();
        for item in self.iter() {
            items.push(item.to_json_value()?);
        }
        Ok(__py_json_value::Array(items))
    }
}

impl<T: PyToJsonValue> PyToJsonValue for Arc<Mutex<Vec<T>>> {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        let items = self.lock().expect("list mutex poisoned");
        items.to_json_value()
    }
}

impl<T: PyToJsonValue> PyToJsonValue for Rc<RefCell<Vec<T>>> {
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        self.borrow().to_json_value()
    }
}

impl<K, V> PyToJsonValue for std::collections::HashMap<K, V>
where
    K: ToString + Eq + std::hash::Hash,
    V: PyToJsonValue,
{
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        let mut object: std::collections::HashMap<String, __py_json_value> =
            std::collections::HashMap::new();
        for (key, value) in self.iter() {
            object.insert(key.to_string(), value.to_json_value()?);
        }
        Ok(__py_json_value::Object(object))
    }
}

impl<K, V> PyToJsonValue for indexmap::IndexMap<K, V>
where
    K: ToString + Eq + std::hash::Hash,
    V: PyToJsonValue,
{
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        let mut object: std::collections::HashMap<String, __py_json_value> =
            std::collections::HashMap::new();
        for (key, value) in self.iter() {
            object.insert(key.to_string(), value.to_json_value()?);
        }
        Ok(__py_json_value::Object(object))
    }
}

impl<K, V> PyToJsonValue for Arc<Mutex<std::collections::HashMap<K, V>>>
where
    K: ToString + Eq + std::hash::Hash,
    V: PyToJsonValue,
{
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        let object = self.lock().expect("dict mutex poisoned");
        object.to_json_value()
    }
}

impl<K, V> PyToJsonValue for Arc<Mutex<indexmap::IndexMap<K, V>>>
where
    K: ToString + Eq + std::hash::Hash,
    V: PyToJsonValue,
{
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        let object = self.lock().expect("dict mutex poisoned");
        object.to_json_value()
    }
}

impl<K, V> PyToJsonValue for Rc<RefCell<indexmap::IndexMap<K, V>>>
where
    K: ToString + Eq + std::hash::Hash,
    V: PyToJsonValue,
{
    fn to_json_value(&self) -> Result<__py_json_value, PyError> {
        self.borrow().to_json_value()
    }
}

fn py_json_escape_string(value: &str) -> String {
    let mut out = String::new();
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn py_json_render_value(value: &__py_json_value) -> String {
    match value {
        __py_json_value::Null => "null".to_string(),
        __py_json_value::Bool(v) => {
            if *v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        __py_json_value::Int(v) => v.to_string(),
        __py_json_value::Float(v) => {
            let mut out = v.to_string();
            if !out.contains('.') && !out.contains('e') && !out.contains('E') {
                out.push_str(".0");
            }
            out
        }
        __py_json_value::Str(v) => py_json_escape_string(v),
        __py_json_value::Array(items) => {
            let rendered: Vec<String> = items.iter().map(py_json_render_value).collect();
            format!("[{}]", rendered.join(", "))
        }
        __py_json_value::Object(entries) => {
            let mut pairs: Vec<(&String, &__py_json_value)> = entries.iter().collect();
            pairs.sort_by(|(a, _), (b, _)| a.cmp(b));
            let rendered: Vec<String> = pairs
                .iter()
                .map(|(key, value)| {
                    format!("{}: {}", py_json_escape_string(key), py_json_render_value(value))
                })
                .collect();
            format!("{{{}}}", rendered.join(", "))
        }
    }
}

fn py_json_skip_ws(chars: &[char], idx: &mut usize) {
    while *idx < chars.len() && chars[*idx].is_whitespace() {
        *idx += 1;
    }
}

fn py_json_error(message: &str) -> PyError {
    PyError::ValueError(message.to_string().into())
}

fn py_json_parse_string(chars: &[char], idx: &mut usize) -> Result<String, PyError> {
    if *idx >= chars.len() || chars[*idx] != '"' {
        return Err(py_json_error("expected string"));
    }
    *idx += 1;
    let mut out = String::new();
    while *idx < chars.len() {
        let ch = chars[*idx];
        *idx += 1;
        if ch == '"' {
            return Ok(out);
        }
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        if *idx >= chars.len() {
            return Err(py_json_error("unterminated escape sequence"));
        }
        let esc = chars[*idx];
        *idx += 1;
        match esc {
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            '/' => out.push('/'),
            'b' => out.push('\u{08}'),
            'f' => out.push('\u{0C}'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            'u' => {
                if *idx + 4 > chars.len() {
                    return Err(py_json_error("incomplete unicode escape"));
                }
                let hex: String = chars[*idx..(*idx + 4)].iter().collect();
                *idx += 4;
                let code = u32::from_str_radix(&hex, 16)
                    .map_err(|_| py_json_error("invalid unicode escape"))?;
                let decoded = char::from_u32(code).ok_or_else(|| py_json_error("invalid unicode scalar"))?;
                out.push(decoded);
            }
            _ => return Err(py_json_error("unsupported escape sequence")),
        }
    }
    Err(py_json_error("unterminated string"))
}

fn py_json_parse_number(chars: &[char], idx: &mut usize) -> Result<__py_json_value, PyError> {
    let start = *idx;
    if chars[*idx] == '-' {
        *idx += 1;
    }
    if *idx >= chars.len() {
        return Err(py_json_error("invalid number"));
    }
    if chars[*idx] == '0' {
        *idx += 1;
    } else if chars[*idx].is_ascii_digit() {
        while *idx < chars.len() && chars[*idx].is_ascii_digit() {
            *idx += 1;
        }
    } else {
        return Err(py_json_error("invalid number"));
    }

    let mut is_float = false;
    if *idx < chars.len() && chars[*idx] == '.' {
        is_float = true;
        *idx += 1;
        if *idx >= chars.len() || !chars[*idx].is_ascii_digit() {
            return Err(py_json_error("invalid float"));
        }
        while *idx < chars.len() && chars[*idx].is_ascii_digit() {
            *idx += 1;
        }
    }
    if *idx < chars.len() && (chars[*idx] == 'e' || chars[*idx] == 'E') {
        is_float = true;
        *idx += 1;
        if *idx < chars.len() && (chars[*idx] == '+' || chars[*idx] == '-') {
            *idx += 1;
        }
        if *idx >= chars.len() || !chars[*idx].is_ascii_digit() {
            return Err(py_json_error("invalid exponent"));
        }
        while *idx < chars.len() && chars[*idx].is_ascii_digit() {
            *idx += 1;
        }
    }

    let raw: String = chars[start..*idx].iter().collect();
    if is_float {
        let parsed = raw
            .parse::<f64>()
            .map_err(|_| py_json_error("invalid floating-point number"))?;
        return Ok(__py_json_value::Float(parsed));
    }
    let parsed = raw
        .parse::<i64>()
        .map_err(|_| py_json_error("invalid integer number"))?;
    Ok(__py_json_value::Int(parsed))
}

fn py_json_parse_value(chars: &[char], idx: &mut usize) -> Result<__py_json_value, PyError> {
    py_json_skip_ws(chars, idx);
    if *idx >= chars.len() {
        return Err(py_json_error("unexpected end of json input"));
    }
    match chars[*idx] {
        'n' => {
            if chars.get(*idx..(*idx + 4)) == Some(&['n', 'u', 'l', 'l']) {
                *idx += 4;
                return Ok(__py_json_value::Null);
            }
            Err(py_json_error("invalid literal, expected null"))
        }
        't' => {
            if chars.get(*idx..(*idx + 4)) == Some(&['t', 'r', 'u', 'e']) {
                *idx += 4;
                return Ok(__py_json_value::Bool(true));
            }
            Err(py_json_error("invalid literal, expected true"))
        }
        'f' => {
            if chars.get(*idx..(*idx + 5)) == Some(&['f', 'a', 'l', 's', 'e']) {
                *idx += 5;
                return Ok(__py_json_value::Bool(false));
            }
            Err(py_json_error("invalid literal, expected false"))
        }
        '"' => Ok(__py_json_value::Str(py_json_parse_string(chars, idx)?)),
        '[' => {
            *idx += 1;
            py_json_skip_ws(chars, idx);
            let mut items: Vec<__py_json_value> = Vec::new();
            if *idx < chars.len() && chars[*idx] == ']' {
                *idx += 1;
                return Ok(__py_json_value::Array(items));
            }
            loop {
                items.push(py_json_parse_value(chars, idx)?);
                py_json_skip_ws(chars, idx);
                if *idx >= chars.len() {
                    return Err(py_json_error("unterminated array"));
                }
                if chars[*idx] == ',' {
                    *idx += 1;
                    py_json_skip_ws(chars, idx);
                    continue;
                }
                if chars[*idx] == ']' {
                    *idx += 1;
                    break;
                }
                return Err(py_json_error("expected ',' or ']' in array"));
            }
            Ok(__py_json_value::Array(items))
        }
        '{' => {
            *idx += 1;
            py_json_skip_ws(chars, idx);
            let mut entries: std::collections::HashMap<String, __py_json_value> =
                std::collections::HashMap::new();
            if *idx < chars.len() && chars[*idx] == '}' {
                *idx += 1;
                return Ok(__py_json_value::Object(entries));
            }
            loop {
                py_json_skip_ws(chars, idx);
                let key = py_json_parse_string(chars, idx)?;
                py_json_skip_ws(chars, idx);
                if *idx >= chars.len() || chars[*idx] != ':' {
                    return Err(py_json_error("expected ':' after object key"));
                }
                *idx += 1;
                let value = py_json_parse_value(chars, idx)?;
                entries.insert(key, value);
                py_json_skip_ws(chars, idx);
                if *idx >= chars.len() {
                    return Err(py_json_error("unterminated object"));
                }
                if chars[*idx] == ',' {
                    *idx += 1;
                    py_json_skip_ws(chars, idx);
                    continue;
                }
                if chars[*idx] == '}' {
                    *idx += 1;
                    break;
                }
                return Err(py_json_error("expected ',' or '}' in object"));
            }
            Ok(__py_json_value::Object(entries))
        }
        '-' | '0'..='9' => py_json_parse_number(chars, idx),
        _ => Err(py_json_error("unsupported json token")),
    }
}
"#;

/// Static helper body for `json.dumps(value)`.
const HELPER_PY_JSON_DUMPS: &str = r#"
fn py_json_dumps<T: PyToJsonValue>(value: &T) -> Result<String, PyError> {
    let json_value = value.to_json_value()?;
    Ok(py_json_render_value(&json_value))
}
"#;

/// Static helper body for `json.loads(text)`.
const HELPER_PY_JSON_LOADS: &str = r#"
fn py_json_loads(text: &str) -> Result<__py_json_value, PyError> {
    let chars: Vec<char> = text.chars().collect();
    let mut idx: usize = 0;
    let value = py_json_parse_value(&chars, &mut idx)?;
    py_json_skip_ws(&chars, &mut idx);
    if idx != chars.len() {
        return Err(py_json_error("trailing data after valid json value"));
    }
    Ok(value)
}
"#;

/// Static helper body for `json.dump(value, file)`.
const HELPER_PY_JSON_DUMP: &str = r#"
fn py_json_dump<T: PyToJsonValue>(value: &T, file: &mut std::fs::File) -> Result<(), PyError> {
    use std::io::Write;
    let text = py_json_dumps(value)?;
    file.write_all(text.as_bytes())
        .map_err(|err| PyError::IOError(err.to_string().into()))
}
"#;

/// Static helper body for `json.load(file)`.
const HELPER_PY_JSON_LOAD: &str = r#"
fn py_json_load(file: &mut std::fs::File) -> Result<__py_json_value, PyError> {
    use std::io::Read;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|err| PyError::IOError(err.to_string().into()))?;
    py_json_loads(&text)
}
"#;

/// Static helper body for clonable iterator wrapper.
const HELPER_PY_ITER: &str = r#"
#[derive(Clone)]
struct PyIter<T> {
    inner: Arc<Mutex<Box<dyn Iterator<Item = T> + Send>>>,
}
impl<T> PyIter<T> {
    fn new<I: Iterator<Item = T> + Send + 'static>(iter: I) -> Self {
        Self { inner: Arc::new(Mutex::new(Box::new(iter))) }
    }
}
impl<T> Iterator for PyIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.lock().expect("PyIter mutex poisoned").next()
    }
}
trait PyIteratorSendClose<T>: Iterator<Item = T> {
    fn send(&mut self, _value: T) -> T {
        self.next()
            .unwrap_or_else(|| panic!("generator is exhausted"))
    }
    fn close(&mut self) {}
}
impl<I, T> PyIteratorSendClose<T> for I where I: Iterator<Item = T> {}
fn py_iter<T, I>(iter: I) -> PyIter<T>
where
    I: Iterator<Item = T> + Send + 'static,
{
    PyIter::new(iter)
}
"#;

/// Static helper body for `len(...)` support across core collection types.
const HELPER_PY_LEN: &str = r#"
trait PyLen {
    fn py_len(&self) -> i64;
}
impl<T> PyLen for Vec<T> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl<T> PyLen for Arc<Mutex<Vec<T>>> {
    fn py_len(&self) -> i64 { self.lock().expect("list mutex poisoned").len() as i64 }
}
impl<T> PyLen for Rc<RefCell<Vec<T>>> {
    fn py_len(&self) -> i64 { self.borrow().len() as i64 }
}
impl PyLen for String {
    fn py_len(&self) -> i64 { self.chars().count() as i64 }
}
impl PyLen for &str {
    fn py_len(&self) -> i64 { self.chars().count() as i64 }
}
impl<K, V> PyLen for indexmap::IndexMap<K, V> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl<K, V> PyLen for Arc<Mutex<indexmap::IndexMap<K, V>>> {
    fn py_len(&self) -> i64 { self.lock().expect("dict mutex poisoned").len() as i64 }
}
impl<K, V> PyLen for Rc<RefCell<indexmap::IndexMap<K, V>>> {
    fn py_len(&self) -> i64 { self.borrow().len() as i64 }
}
impl<T> PyLen for std::collections::HashSet<T> {
    fn py_len(&self) -> i64 { self.len() as i64 }
}
impl PyLen for () {
    fn py_len(&self) -> i64 { 0 }
}
impl<T1> PyLen for (T1,) {
    fn py_len(&self) -> i64 { 1 }
}
impl<T1, T2> PyLen for (T1, T2) {
    fn py_len(&self) -> i64 { 2 }
}
impl<T1, T2, T3> PyLen for (T1, T2, T3) {
    fn py_len(&self) -> i64 { 3 }
}
impl<T1, T2, T3, T4> PyLen for (T1, T2, T3, T4) {
    fn py_len(&self) -> i64 { 4 }
}
impl<T1, T2, T3, T4, T5> PyLen for (T1, T2, T3, T4, T5) {
    fn py_len(&self) -> i64 { 5 }
}
impl<T1, T2, T3, T4, T5, T6> PyLen for (T1, T2, T3, T4, T5, T6) {
    fn py_len(&self) -> i64 { 6 }
}
impl<T1, T2, T3, T4, T5, T6, T7> PyLen for (T1, T2, T3, T4, T5, T6, T7) {
    fn py_len(&self) -> i64 { 7 }
}
impl<T1, T2, T3, T4, T5, T6, T7, T8> PyLen for (T1, T2, T3, T4, T5, T6, T7, T8) {
    fn py_len(&self) -> i64 { 8 }
}
fn py_len<T: PyLen>(v: &T) -> i64 { v.py_len() }
"#;

/// Static helper body for list slicing with arbitrary step.
const HELPER_PY_LIST_SLICE_STEP: &str = r#"
fn py_list_slice_step<T: Clone>(items: &[T], start: Option<i64>, end: Option<i64>, step: i64) -> Result<Vec<T>, PyError> {
    if step == 0 { return Err(PyError::ValueError("slice step cannot be zero".into())); }
    let len = items.len() as i64;
    let mut out = Vec::new();
    if step > 0 {
        let mut i = match start {
            Some(s) => { let s = if s < 0 { len + s } else { s }; s.max(0).min(len) },
            None => 0,
        };
        let end = match end {
            Some(e) => { let e = if e < 0 { len + e } else { e }; e.max(0).min(len) },
            None => len,
        };
        while i < end {
            out.push(items[i as usize].clone());
            i += step;
        }
    } else {
        let mut i = match start {
            Some(s) => { let s = if s < 0 { len + s } else { s }; if s < 0 { -1 } else if s >= len { len - 1 } else { s } },
            None => len - 1,
        };
        let end = match end {
            Some(e) => { let e = if e < 0 { len + e } else { e }; if e < 0 { -1 } else if e >= len { len - 1 } else { e } },
            None => -1,
        };
        while i > end {
            if i >= 0 && i < len { out.push(items[i as usize].clone()); }
            i += step;
        }
    }
    Ok(out)
}
"#;

/// Static helper body for string slicing with arbitrary step.
const HELPER_PY_STR_SLICE_STEP: &str = r#"
fn py_str_slice_step(s: &str, start: Option<i64>, end: Option<i64>, step: i64) -> Result<String, PyError> {
    let chars: Vec<char> = s.chars().collect();
    let sliced = py_list_slice_step(&chars, start, end, step)?;
    Ok(sliced.into_iter().collect())
}
"#;

/// Static helper body for file open/read/write helpers.
const HELPER_PY_FILE: &str = r#"
fn py_open(path: &str, mode: &str) -> Result<std::fs::File, PyError> {
    match mode {
        "r" => std::fs::File::open(path).map_err(|e| PyError::IOError(e.to_string().into())),
        "w" => std::fs::OpenOptions::new().create(true).write(true).truncate(true).open(path).map_err(|e| PyError::IOError(e.to_string().into())),
        "a" => std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| PyError::IOError(e.to_string().into())),
        _ => Err(PyError::ValueError(format!("unsupported file mode: {}", mode).into())),
    }
}
fn py_file_read(file: &mut std::fs::File, n: Option<i64>) -> Result<String, PyError> {
    use std::io::Read;
    if let Some(limit) = n {
        if limit >= 0 {
            let mut buf = vec![0u8; limit as usize];
            let read = file.read(&mut buf).map_err(|e| PyError::IOError(e.to_string().into()))?;
            buf.truncate(read);
            return String::from_utf8(buf).map_err(|e| PyError::IOError(e.to_string().into()));
        }
    }
    let mut out = String::new();
    file.read_to_string(&mut out).map_err(|e| PyError::IOError(e.to_string().into()))?;
    Ok(out)
}
fn py_file_readline(file: &mut std::fs::File) -> Result<String, PyError> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = file.read(&mut byte).map_err(|e| PyError::IOError(e.to_string().into()))?;
        if read == 0 { break; }
        bytes.push(byte[0]);
        if byte[0] == b'\n' { break; }
    }
    String::from_utf8(bytes).map_err(|e| PyError::IOError(e.to_string().into()))
}
fn py_file_readlines(file: &mut std::fs::File) -> Result<Vec<String>, PyError> {
    let mut lines = Vec::new();
    loop {
        let line = py_file_readline(file)?;
        if line.is_empty() { break; }
        lines.push(line);
    }
    Ok(lines)
}
fn py_file_write(file: &mut std::fs::File, data: &str) -> Result<i64, PyError> {
    use std::io::Write;
    file.write_all(data.as_bytes()).map_err(|e| PyError::IOError(e.to_string().into()))?;
    Ok(data.len() as i64)
}
fn py_file_close(file: &mut std::fs::File) -> Result<(), PyError> {
    use std::io::Write;
    file.flush().map_err(|e| PyError::IOError(e.to_string().into()))
}
"#;

/// Static helper body for Python-compatible float string formatting.
const HELPER_PY_FLOAT_STR: &str = r#"
fn py_float_str(v: f64) -> String {
    if v.is_nan() { return "nan".to_string(); }
    if v.is_infinite() {
        return if v.is_sign_negative() { "-inf".to_string() } else { "inf".to_string() };
    }
    let mut s = v.to_string();
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    s
}
"#;

/// Static helper body for Python-style string repr escaping.
const HELPER_PY_STR_REPR: &str = r#"
fn py_str_repr(s: &str) -> String {
    let use_double = s.contains('\'') && !s.contains('"');
    let quote = if use_double { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\x08"),
            '\x0c' => out.push_str("\\x0c"),
            '\'' if quote == '\'' => out.push_str("\\'"),
            '"' if quote == '"' => out.push_str("\\\""),
            _ => out.push(ch),
        }
    }
    out.push(quote);
    out
}
"#;

/// Static helper body for Python `ascii(...)` escaping.
const HELPER_PY_ASCII: &str = r#"
fn py_ascii_escape(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii() {
            out.push(ch);
            continue;
        }
        let code = ch as u32;
        if code <= 0xFF {
            out.push_str(&format!("\\x{:02x}", code));
        } else if code <= 0xFFFF {
            out.push_str(&format!("\\u{:04x}", code));
        } else {
            out.push_str(&format!("\\U{:08x}", code));
        }
    }
    out
}
"#;

/// Static helper body for Python string method compatibility.
const HELPER_PY_STRING_METHODS: &str = r#"
fn py_str_split_whitespace(s: &str, maxsplit: Option<i64>) -> Vec<String> {
    let maxsplit = maxsplit.unwrap_or(-1);
    if maxsplit < 0 {
        return s.split_whitespace().map(|part| part.to_string()).collect();
    }
    let limit = maxsplit as usize;
    if limit == 0 {
        let trimmed = s.trim_start();
        if trimmed.is_empty() {
            return Vec::new();
        }
        return vec![trimmed.to_string()];
    }
    let mut raw: Vec<String> = s.split_whitespace().map(|part| part.to_string()).collect();
    if raw.len() <= limit + 1 {
        return raw;
    }
    let mut out: Vec<String> = raw.drain(..limit).collect();
    out.push(raw.join(" "));
    out
}

fn py_str_split_sep(s: &str, sep: &str, maxsplit: Option<i64>) -> Vec<String> {
    if maxsplit.unwrap_or(-1) < 0 {
        return s.split(sep).map(|part| part.to_string()).collect();
    }
    let limit = maxsplit.unwrap_or(-1).max(0) as usize + 1;
    s.splitn(limit, sep).map(|part| part.to_string()).collect()
}

fn py_str_count(s: &str, needle: &str) -> i64 {
    if needle.is_empty() {
        return s.chars().count() as i64 + 1;
    }
    s.matches(needle).count() as i64
}

fn py_str_title(s: &str) -> String {
    let mut out = String::new();
    let mut at_word_start = true;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            if at_word_start {
                for upper in ch.to_uppercase() {
                    out.push(upper);
                }
                at_word_start = false;
            } else {
                for lower in ch.to_lowercase() {
                    out.push(lower);
                }
            }
        } else {
            at_word_start = true;
            out.push(ch);
        }
    }
    out
}

fn py_str_capitalize(s: &str) -> String {
    let mut chars = s.chars();
    let mut out = String::new();
    if let Some(first) = chars.next() {
        for upper in first.to_uppercase() {
            out.push(upper);
        }
        for ch in chars {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        }
    }
    out
}

fn py_str_swapcase(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_lowercase() {
            for upper in ch.to_uppercase() {
                out.push(upper);
            }
        } else if ch.is_uppercase() {
            for lower in ch.to_lowercase() {
                out.push(lower);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn py_str_lstrip_chars(s: &str, chars: &str) -> String {
    if chars.is_empty() {
        return s.to_string();
    }
    let mut start = 0usize;
    for (idx, ch) in s.char_indices() {
        if chars.contains(ch) {
            start = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    s[start..].to_string()
}

fn py_str_rstrip_chars(s: &str, chars: &str) -> String {
    if chars.is_empty() {
        return s.to_string();
    }
    let mut end = s.len();
    for (idx, ch) in s.char_indices().rev() {
        if chars.contains(ch) {
            end = idx;
        } else {
            break;
        }
    }
    s[..end].to_string()
}

fn py_fill_char(fill: &str) -> char {
    fill.chars().next().unwrap_or(' ')
}

fn py_str_center(s: &str, width: i64, fill: char) -> String {
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let pad = (width - len) as usize;
    let left = pad / 2;
    let right = pad - left;
    let fill_s = fill.to_string();
    format!("{}{}{}", fill_s.repeat(left), s, fill_s.repeat(right))
}

fn py_str_ljust(s: &str, width: i64, fill: char) -> String {
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let pad = (width - len) as usize;
    let fill_s = fill.to_string();
    format!("{}{}", s, fill_s.repeat(pad))
}

fn py_str_rjust(s: &str, width: i64, fill: char) -> String {
    let len = s.chars().count() as i64;
    if width <= len {
        return s.to_string();
    }
    let pad = (width - len) as usize;
    let fill_s = fill.to_string();
    format!("{}{}", fill_s.repeat(pad), s)
}

fn py_str_zfill(s: &str, width: i64) -> String {
    let width = width.max(0) as usize;
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let pad = width - len;
    if let Some(first) = s.chars().next() {
        if first == '+' || first == '-' {
            let mut out = String::new();
            out.push(first);
            out.push_str(&"0".repeat(pad));
            out.push_str(&s[first.len_utf8()..]);
            return out;
        }
    }
    format!("{}{}", "0".repeat(pad), s)
}

fn py_str_isdigit(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_ascii_digit())
}

fn py_str_isalpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_alphabetic())
}

fn py_str_isalnum(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_alphanumeric())
}

fn py_str_isspace(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|ch| ch.is_whitespace())
}

fn py_str_isupper(s: &str) -> bool {
    let mut has_cased = false;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            has_cased = true;
            if ch.is_lowercase() {
                return false;
            }
        }
    }
    has_cased
}

fn py_str_islower(s: &str) -> bool {
    let mut has_cased = false;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            has_cased = true;
            if ch.is_uppercase() {
                return false;
            }
        }
    }
    has_cased
}
"#;

/// Static helper body for `range(start, end, step)`.
const HELPER_PY_RANGE3: &str = r#"
fn py_range3(start: i64, end: i64, step: i64) -> Result<Box<dyn Iterator<Item = i64>>, PyError> {
    if step == 0 { return Err(PyError::ValueError("range() arg 3 must not be zero".into())); }
    if step > 0 {
        Ok(Box::new((start..end).step_by(step as usize)))
    } else {
        let step = (-step) as usize;
        if start <= end {
            Ok(Box::new(std::iter::empty::<i64>()))
        } else {
            Ok(Box::new(((end + 1)..=start).rev().step_by(step)))
        }
    }
}
"#;

/// Static helper body for Python-style banker rounding.
const HELPER_PY_ROUND: &str = r#"
fn py_round_ties_even(value: f64) -> f64 {
    let rounded = value.round();
    let diff = (value - rounded).abs();
    if (diff - 0.5).abs() < 1e-12 {
        let floor = value.floor();
        if (floor as i64) % 2 == 0 { floor } else { floor + 1.0 }
    } else {
        rounded
    }
}
fn py_round(value: f64, ndigits: i64) -> f64 {
    let factor = 10f64.powi(ndigits as i32);
    py_round_ties_even(value * factor) / factor
}
"#;

/// Static helper body for `max(...)` over iterables.
const HELPER_PY_MAX: &str = r#"
fn py_max<T: PartialOrd, I: IntoIterator<Item = T>>(iter: I) -> Result<T, PyError> {
    let mut iter = iter.into_iter();
    let mut best = iter.next().ok_or_else(|| PyError::ValueError("max() arg is an empty sequence".into()))?;
    for item in iter {
        if item.partial_cmp(&best).unwrap_or(std::cmp::Ordering::Equal) == std::cmp::Ordering::Greater {
            best = item;
        }
    }
    Ok(best)
}
"#;

/// Static helper body for `min(...)` over iterables.
const HELPER_PY_MIN: &str = r#"
fn py_min<T: PartialOrd, I: IntoIterator<Item = T>>(iter: I) -> Result<T, PyError> {
    let mut iter = iter.into_iter();
    let mut best = iter.next().ok_or_else(|| PyError::ValueError("min() arg is an empty sequence".into()))?;
    for item in iter {
        if item.partial_cmp(&best).unwrap_or(std::cmp::Ordering::Equal) == std::cmp::Ordering::Less {
            best = item;
        }
    }
    Ok(best)
}
"#;

/// Static helper body for `list` string representation helpers (part 1).
const HELPER_PY_LIST_REPR_HEAD: &str = r#"
trait PyListRepr {
    fn py_repr(&self) -> String;
}
impl PyListRepr for i64 {
    fn py_repr(&self) -> String { self.to_string() }
}
impl PyListRepr for f64 {
    fn py_repr(&self) -> String { py_float_str(*self) }
}
impl PyListRepr for bool {
    fn py_repr(&self) -> String { if *self { "True".to_string() } else { "False".to_string() } }
}
impl PyListRepr for String {
    fn py_repr(&self) -> String { py_str_repr(self) }
}
"#;

/// Static helper body for `PyRepr` list representation support.
const HELPER_PY_LIST_REPR_PY_REPR: &str = r#"
impl PyListRepr for PyRepr {
    fn py_repr(&self) -> String { self.0.clone() }
}
"#;

/// Static helper body for `list` string representation helpers (part 2).
const HELPER_PY_LIST_REPR_TAIL: &str = r#"
impl<T1: PyListRepr> PyListRepr for (T1,) {
    fn py_repr(&self) -> String { format!("({},)", self.0.py_repr()) }
}
impl<T1: PyListRepr, T2: PyListRepr> PyListRepr for (T1, T2) {
    fn py_repr(&self) -> String { format!("({}, {})", self.0.py_repr(), self.1.py_repr()) }
}
impl<T1: PyListRepr, T2: PyListRepr, T3: PyListRepr> PyListRepr for (T1, T2, T3) {
    fn py_repr(&self) -> String { format!("({}, {}, {})", self.0.py_repr(), self.1.py_repr(), self.2.py_repr()) }
}
impl<T1: PyListRepr, T2: PyListRepr, T3: PyListRepr, T4: PyListRepr> PyListRepr for (T1, T2, T3, T4) {
    fn py_repr(&self) -> String { format!("({}, {}, {}, {})", self.0.py_repr(), self.1.py_repr(), self.2.py_repr(), self.3.py_repr()) }
}
impl<T: PyListRepr> PyListRepr for Vec<T> {
    fn py_repr(&self) -> String { py_list_str_vec(self) }
}
impl<T: PyListRepr> PyListRepr for Arc<Mutex<Vec<T>>> {
    fn py_repr(&self) -> String { py_list_str(self) }
}
impl<T: PyListRepr> PyListRepr for Rc<RefCell<Vec<T>>> {
    fn py_repr(&self) -> String { py_list_str_rc(self) }
}
fn py_list_str_vec<T: PyListRepr>(list: &[T]) -> String {
    let mut out = "[".to_string();
    for (idx, item) in list.iter().enumerate() {
        if idx > 0 { out.push_str(", "); }
        out.push_str(&item.py_repr());
    }
    out.push(']');
    out
}
fn py_list_str<T: PyListRepr>(list: &Arc<Mutex<Vec<T>>>) -> String {
    let guard = list.lock().expect("list mutex poisoned");
    py_list_str_vec(&guard)
}
fn py_list_str_rc<T: PyListRepr>(list: &Rc<RefCell<Vec<T>>>) -> String {
    let guard = list.borrow();
    py_list_str_vec(&guard)
}
"#;

/// Static helper body for PyError enum and trait impls.
const HELPER_PY_ERROR_ENUM: &str = r#"
pub type PyErrorMsg = std::borrow::Cow<'static, str>;

#[derive(Debug, Clone)]
pub enum PyError {
    Exception(PyErrorMsg),
    ValueError(PyErrorMsg),
    TypeError(PyErrorMsg),
    RuntimeError(PyErrorMsg),
    KeyError(PyErrorMsg),
    IndexError(PyErrorMsg),
    AttributeError(PyErrorMsg),
    ZeroDivisionError(PyErrorMsg),
    NameError(PyErrorMsg),
    AssertionError(PyErrorMsg),
    StopIteration(PyErrorMsg),
    NotImplementedError(PyErrorMsg),
    IOError(PyErrorMsg),
    OverflowError(PyErrorMsg),
    GeneratorExit(PyErrorMsg),
    MemoryError(PyErrorMsg),
}

impl std::fmt::Display for PyError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PyError::Exception(msg) => write!(f, "Exception: {}", msg),
            PyError::ValueError(msg) => write!(f, "ValueError: {}", msg),
            PyError::TypeError(msg) => write!(f, "TypeError: {}", msg),
            PyError::RuntimeError(msg) => write!(f, "RuntimeError: {}", msg),
            PyError::KeyError(msg) => write!(f, "KeyError: {}", msg),
            PyError::IndexError(msg) => write!(f, "IndexError: {}", msg),
            PyError::AttributeError(msg) => write!(f, "AttributeError: {}", msg),
            PyError::ZeroDivisionError(msg) => write!(f, "ZeroDivisionError: {}", msg),
            PyError::NameError(msg) => write!(f, "NameError: {}", msg),
            PyError::AssertionError(msg) => write!(f, "AssertionError: {}", msg),
            PyError::StopIteration(msg) => write!(f, "StopIteration: {}", msg),
            PyError::NotImplementedError(msg) => write!(f, "NotImplementedError: {}", msg),
            PyError::IOError(msg) => write!(f, "IOError: {}", msg),
            PyError::OverflowError(msg) => write!(f, "OverflowError: {}", msg),
            PyError::GeneratorExit(msg) => write!(f, "GeneratorExit: {}", msg),
            PyError::MemoryError(msg) => write!(f, "MemoryError: {}", msg),
        }
    }
}

impl std::error::Error for PyError {}
"#;

impl<'a> Codegen<'a> {
    /// Emit a raw helper block line-by-line with current indentation.
    ///
    /// The input block should be left-aligned Rust source.
    fn push_block(&mut self, block: &str) {
        for line in block.trim_matches('\n').lines() {
            self.push_line(line);
        }
    }

    /// Emit all helper functions needed by the generated code.
    ///
    /// Helpers are only emitted if their corresponding `uses.*` flag is set.
    /// This is determined by scanning the HIR before code generation.
    ///
    /// Why inject helpers instead of using a runtime crate?
    /// 1. Self-contained output: one .rs file can be compiled standalone
    /// 2. No version dependencies: no need to manage crate versions
    /// 3. Easier distribution: users can just copy the .rs file
    /// 4. Transparency: users can see exactly what code is running
    ///
    /// Drawbacks:
    /// - Larger generated files if many helpers are used
    /// - Code duplication if transpiling multiple Python files
    ///
    /// We accept these tradeoffs because py2rust targets single-file transpilation.
    pub(crate) fn emit_helpers(&mut self) {
        // List/tuple repr relies on shared float/string repr helpers.
        if self.uses.py_list_str {
            self.uses.py_float_str = true;
            self.uses.py_str_repr = true;
        }
        // `re.Match` storage is shared by all lightweight `re` entrypoints.
        let uses_re_match_struct = self.uses.py_re_search
            || self.uses.py_re_match
            || self.uses.py_re_sub
            || self.uses.py_re_group
            || self.uses.py_re_span;
        // Shared span-finding utilities are required for search/match only.
        let uses_re_core = self.uses.py_re_search || self.uses.py_re_match;
        // `json` runtime shares a value model and parser/serializer core.
        let uses_json_core = self.uses.py_json_dumps
            || self.uses.py_json_loads
            || self.uses.py_json_dump
            || self.uses.py_json_load;
        // `time` tuple conversions/parsing are shared by localtime/gmtime/strftime/strptime.
        let uses_time_core = self.uses.py_time_localtime
            || self.uses.py_time_gmtime
            || self.uses.py_time_strftime
            || self.uses.py_time_strptime;
        // urllib.parse result objects are shared by urlparse/urljoin/geturl.
        let uses_urllib_parse_result_struct = self.uses.py_urllib_urlparse
            || self.uses.py_urllib_parse_geturl
            || self.uses.py_urllib_urljoin;
        // urllib.request responses are shared by urlopen and response accessors.
        let uses_urllib_response_struct = self.uses.py_urllib_urlopen
            || self.uses.py_urllib_response_read
            || self.uses.py_urllib_response_getcode
            || self.uses.py_urllib_response_geturl;

        // PyError enum is needed for exception handling.
        if self.needs_py_error() {
            self.emit_py_error_enum();
        }

        // PyIter wrapper makes iterators clonable (Python's for loops clone iterators).
        if self.uses.py_iter {
            // Poisoned mutex means iterator state is invalid; panic with context.
            self.push_block(HELPER_PY_ITER);
        }

        if self.uses.print {
            self.push_block(HELPER_PY_PRINT);
        }
        if self.uses.py_int {
            self.push_block(HELPER_PY_INT);
        }
        if self.uses.py_repr {
            // PyRepr wraps preformatted strings so list Debug output matches Python repr style.
            self.push_line("#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]");
            self.push_line("struct PyRepr(String);");
            self.push_line("impl std::fmt::Debug for PyRepr {");
            self.indent += 1;
            self.push_line("fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {");
            self.indent += 1;
            self.push_line("write!(f, \"{}\", self.0)");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_float_str {
            self.push_block(HELPER_PY_FLOAT_STR);
        }
        if self.uses.py_str_repr {
            self.push_block(HELPER_PY_STR_REPR);
        }
        if self.uses.py_ascii {
            self.push_block(HELPER_PY_ASCII);
        }
        if self.uses.py_string_methods {
            self.push_block(HELPER_PY_STRING_METHODS);
        }
        if self.uses.len {
            // Tuples have fixed arity, so provide PyLen impls for common tuple sizes.
            self.push_block(HELPER_PY_LEN);
        }
        if self.uses.range {
            self.push_line("fn py_range(end: i64) -> std::ops::Range<i64> { 0..end }");
        }
        if self.uses.range2 {
            self.push_line(
                "fn py_range2(start: i64, end: i64) -> std::ops::Range<i64> { start..end }",
            );
        }
        if self.uses.range3 {
            // range(start, end, step) with arbitrary step values.
            // Positive step: count up from start to end.
            // Negative step: count down from start to end.
            // Step of 0 is an error (would create infinite loop).
            self.push_block(HELPER_PY_RANGE3);
        }
        if self.uses.round {
            // Python's round() uses "round half to even" (banker's rounding).
            // Unlike Rust's f64::round() which uses "round half away from zero".
            // Example: round(0.5) = 0, round(1.5) = 2, round(2.5) = 2
            // This minimizes bias in repeated rounding operations.
            self.push_block(HELPER_PY_ROUND);
        }
        if self.uses.type_name {
            self.push_block(HELPER_PY_TYPE_NAME);
        }
        if self.uses.py_max {
            // Use PartialOrd to support floats and fall back to equality on NaN comparisons.
            self.push_block(HELPER_PY_MAX);
        }
        if self.uses.py_min {
            // Use PartialOrd to support floats and fall back to equality on NaN comparisons.
            self.push_block(HELPER_PY_MIN);
        }
        if self.uses.py_parse_int {
            self.push_line("fn py_parse_int(s: &str) -> Result<i64, PyError> {");
            self.indent += 1;
            self.push_line("let trimmed = s.trim();");
            self.push_line(
                "trimmed.parse().map_err(|_| PyError::ValueError(format!(\"invalid literal for int() with base 10: '{}'\", trimmed).into()))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_parse_float {
            self.push_line("fn py_parse_float(s: &str) -> Result<f64, PyError> {");
            self.indent += 1;
            self.push_line("let trimmed = s.trim();");
            self.push_line(
                "trimmed.parse().map_err(|_| PyError::ValueError(format!(\"could not convert string to float: '{}'\", trimmed).into()))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_bytes_from_len {
            // Match Python bytes(n): negative sizes raise ValueError.
            self.push_line("fn py_bytes_from_len(len: i64) -> Result<Vec<i64>, PyError> {");
            self.indent += 1;
            self.push_line("if len < 0 {");
            self.indent += 1;
            self.push_line("return Err(PyError::ValueError(\"negative count\".into()));");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(vec![0i64; len as usize])");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_bytes_from_str {
            // Only UTF-8 encoding is supported for bytes(str, encoding).
            self.push_line(
                "fn py_bytes_from_str(s: &str, encoding: &str) -> Result<Vec<i64>, PyError> {",
            );
            self.indent += 1;
            self.push_line("if encoding != \"utf-8\" {");
            self.indent += 1;
            self.push_line("return Err(PyError::ValueError(\"unsupported encoding\".into()));");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Ok(s.as_bytes().iter().map(|b| *b as i64).collect())");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_chr {
            self.push_line("fn py_chr(value: i64) -> Result<String, PyError> {");
            self.indent += 1;
            self.push_line("if value < 0 || value > 0x10FFFF {");
            self.indent += 1;
            self.push_line(
                "return Err(PyError::ValueError(\"chr() arg not in range(0x110000)\".into()));",
            );
            self.indent -= 1;
            self.push_line("}");
            self.push_line("match std::char::from_u32(value as u32) {");
            self.indent += 1;
            self.push_line("Some(c) => Ok(c.to_string()),");
            self.push_line(
                "None => Err(PyError::ValueError(\"chr() arg not in range(0x110000)\".into())),",
            );
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_ord {
            self.push_line("fn py_ord(s: &str) -> Result<i64, PyError> {");
            self.indent += 1;
            self.push_line("let mut chars = s.chars();");
            self.push_line("match (chars.next(), chars.next()) {");
            self.indent += 1;
            self.push_line("(Some(c), None) => Ok(c as i64),");
            self.push_line("_ => Err(PyError::TypeError(\"ord() expects a character\".into())),");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_next {
            self.push_block(HELPER_PY_NEXT);
        }
        if self.uses.py_dict_get {
            self.push_line(
                "fn py_dict_get<K: Eq + std::hash::Hash, V: Clone>(map: &IndexMap<K, V>, key: &K) -> Result<V, PyError> {",
            );
            self.indent += 1;
            self.push_line(
                "map.get(key).cloned().ok_or_else(|| PyError::KeyError(\"KeyError\".into()))",
            );
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_get {
            self.push_line(
                "fn py_list_get<T: Clone>(items: &[T], idx: i64) -> Result<T, PyError> {",
            );
            self.indent += 1;
            self.push_line("let len = items.len() as i64;");
            self.push_line("let adj = if idx < 0 { len + idx } else { idx };");
            self.push_line("if adj < 0 || adj >= len {");
            self.indent += 1;
            self.push_line("Err(PyError::IndexError(\"IndexError\".into()))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("Ok(items[adj as usize].clone())");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_str_get {
            self.push_block(HELPER_PY_STR_GET);
        }
        if self.uses.py_list_index {
            self.push_line(
                "fn py_list_index<T: PartialEq>(items: &[T], needle: &T) -> Result<i64, PyError> {",
            );
            self.indent += 1;
            self.push_line("for (idx, item) in items.iter().enumerate() {");
            self.indent += 1;
            self.push_line("if item == needle { return Ok(idx as i64); }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("Err(PyError::ValueError(\"ValueError\".into()))");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_count {
            self.push_line("fn py_list_count<T: PartialEq>(items: &[T], needle: &T) -> i64 {");
            self.indent += 1;
            self.push_line("let mut count = 0i64;");
            self.push_line("for item in items.iter() {");
            self.indent += 1;
            self.push_line("if item == needle { count += 1; }");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("count");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_str {
            self.push_block(HELPER_PY_LIST_REPR_HEAD);
            if self.uses.py_repr {
                self.push_block(HELPER_PY_LIST_REPR_PY_REPR);
            }
            self.push_block(HELPER_PY_LIST_REPR_TAIL);
        }
        if self.uses.py_index {
            // Python supports negative indices: list[-1] is the last element.
            // Formula: negative index i becomes len + i.
            // Example: for list of length 5, index -1 becomes 5 + (-1) = 4.
            self.push_line("fn py_index(idx: i64, len: usize) -> Result<usize, PyError> {");
            self.indent += 1;
            self.push_line("let len_i = len as i64;");
            self.push_line("let adj = if idx < 0 { len_i + idx } else { idx };");
            self.push_line("if adj < 0 || adj >= len_i {");
            self.indent += 1;
            self.push_line("Err(PyError::IndexError(\"IndexError\".into()))");
            self.indent -= 1;
            self.push_line("} else {");
            self.indent += 1;
            self.push_line("Ok(adj as usize)");
            self.indent -= 1;
            self.push_line("}");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_insert_index {
            // Python list.insert clamps indices: <0 inserts at start, >len inserts at end.
            self.push_line("fn py_insert_index(idx: i64, len: usize) -> usize {");
            self.indent += 1;
            self.push_line("let len_i = len as i64;");
            self.push_line("let mut adj = if idx < 0 { len_i + idx } else { idx };");
            self.push_line("if adj < 0 { adj = 0; }");
            self.push_line("if adj > len_i { adj = len_i; }");
            self.push_line("adj as usize");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_str_slice {
            self.push_line(
                "fn py_str_slice(s: &str, start: Option<i64>, end: Option<i64>) -> String {",
            );
            self.indent += 1;
            self.push_line("let chars: Vec<char> = s.chars().collect();");
            self.push_line("let len = chars.len() as i64;");
            self.push_line("let start = start.map(|i| if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(0) as usize;");
            self.push_line("let end = end.map(|i| if i < 0 { (len + i).max(0) } else { i.min(len) }).unwrap_or(len as i64) as usize;");
            self.push_line("chars[start..end.max(start)].iter().collect()");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_list_slice_step {
            self.push_block(HELPER_PY_LIST_SLICE_STEP);
        }
        if self.uses.py_str_slice_step {
            self.push_block(HELPER_PY_STR_SLICE_STEP);
        }
        if self.uses.py_file {
            self.push_block(HELPER_PY_FILE);
        }
        if self.uses.py_input {
            self.push_line("fn py_input(prompt: Option<&str>) -> String {");
            self.indent += 1;
            self.push_line("use std::io::{self, Write};");
            self.push_line("if let Some(prompt) = prompt {");
            self.indent += 1;
            self.push_line("print!(\"{}\", prompt);");
            self.push_line("io::stdout().flush().expect(\"stdout flush failed\");");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("let mut line = String::new();");
            self.push_line("io::stdin().read_line(&mut line).expect(\"stdin read failed\");");
            self.push_line("while matches!(line.chars().last(), Some('\\n' | '\\r')) {");
            self.indent += 1;
            self.push_line("line.pop();");
            self.indent -= 1;
            self.push_line("}");
            self.push_line("line");
            self.indent -= 1;
            self.push_line("}");
        }
        if self.uses.py_os_remove {
            self.push_block(HELPER_PY_OS_REMOVE);
        }
        if self.uses.py_os_getcwd {
            self.push_block(HELPER_PY_OS_GETCWD);
        }
        if self.uses.py_os_chdir {
            self.push_block(HELPER_PY_OS_CHDIR);
        }
        if self.uses.py_os_mkdir {
            self.push_block(HELPER_PY_OS_MKDIR);
        }
        if self.uses.py_os_listdir {
            self.push_block(HELPER_PY_OS_LISTDIR);
        }
        if self.uses.py_os_rmdir {
            self.push_block(HELPER_PY_OS_RMDIR);
        }
        if self.uses.py_os_rename {
            self.push_block(HELPER_PY_OS_RENAME);
        }
        if self.uses.py_os_replace {
            self.push_block(HELPER_PY_OS_REPLACE);
        }
        if self.uses.py_os_makedirs {
            self.push_block(HELPER_PY_OS_MAKEDIRS);
        }
        if self.uses.py_os_getenv {
            self.push_block(HELPER_PY_OS_GETENV);
        }
        if self.uses.py_os_environ {
            self.push_block(HELPER_PY_OS_ENVIRON);
        }
        if self.uses.py_os_name {
            self.push_block(HELPER_PY_OS_NAME);
        }
        if self.uses.py_os_path_join {
            self.push_block(HELPER_PY_OS_PATH_JOIN);
        }
        if self.uses.py_os_path_exists {
            self.push_block(HELPER_PY_OS_PATH_EXISTS);
        }
        if self.uses.py_os_path_is_file {
            self.push_block(HELPER_PY_OS_PATH_IS_FILE);
        }
        if self.uses.py_os_path_is_dir {
            self.push_block(HELPER_PY_OS_PATH_IS_DIR);
        }
        if self.uses.py_os_path_basename {
            self.push_block(HELPER_PY_OS_PATH_BASENAME);
        }
        if self.uses.py_os_path_dirname {
            self.push_block(HELPER_PY_OS_PATH_DIRNAME);
        }
        if self.uses.py_os_path_split {
            self.push_block(HELPER_PY_OS_PATH_SPLIT);
        }
        if self.uses.py_os_path_abspath {
            self.push_block(HELPER_PY_OS_PATH_ABSPATH);
        }
        if self.uses.py_sys_argv {
            self.push_block(HELPER_PY_SYS_ARGV);
        }
        if self.uses.py_sys_intern {
            self.push_block(HELPER_PY_SYS_INTERN);
        }
        if self.uses.py_subprocess_run {
            self.push_block(HELPER_PY_SUBPROCESS_COMPLETED_PROCESS);
            self.push_block(HELPER_PY_SUBPROCESS_RUN);
        }
        if uses_urllib_parse_result_struct {
            self.push_block(HELPER_PY_URLLIB_PARSE_RESULT_STRUCT);
        }
        if uses_urllib_response_struct {
            self.push_block(HELPER_PY_URLLIB_RESPONSE_STRUCT);
        }
        if self.uses.py_urllib_unquote {
            self.push_block(HELPER_PY_URLLIB_UNQUOTE);
        }
        if self.uses.py_urllib_quote {
            self.push_block(HELPER_PY_URLLIB_QUOTE);
        }
        if self.uses.py_urllib_urlparse {
            self.push_block(HELPER_PY_URLLIB_URLPARSE);
        }
        if self.uses.py_urllib_parse_geturl {
            self.push_block(HELPER_PY_URLLIB_PARSE_GETURL);
        }
        if self.uses.py_urllib_urljoin {
            self.push_block(HELPER_PY_URLLIB_URLJOIN);
        }
        if self.uses.py_urllib_urlencode {
            self.push_block(HELPER_PY_URLLIB_URLENCODE);
        }
        if self.uses.py_urllib_parse_qs {
            self.push_block(HELPER_PY_URLLIB_PARSE_QS);
        }
        if self.uses.py_urllib_urlopen {
            self.push_block(HELPER_PY_URLLIB_URLOPEN);
        }
        if self.uses.py_urllib_response_read {
            self.push_block(HELPER_PY_URLLIB_RESPONSE_READ);
        }
        if self.uses.py_urllib_response_getcode {
            self.push_block(HELPER_PY_URLLIB_RESPONSE_GETCODE);
        }
        if self.uses.py_urllib_response_geturl {
            self.push_block(HELPER_PY_URLLIB_RESPONSE_GETURL);
        }
        if self.uses.py_math_factorial {
            self.push_block(HELPER_PY_MATH_FACTORIAL);
        }
        if self.uses.py_math_gcd {
            self.push_block(HELPER_PY_MATH_GCD);
        }
        if self.uses.py_math_lcm {
            self.push_block(HELPER_PY_MATH_LCM);
        }
        if self.uses.py_math_comb {
            self.push_block(HELPER_PY_MATH_COMB);
        }
        if self.uses.py_math_perm {
            self.push_block(HELPER_PY_MATH_PERM);
        }
        if self.uses.py_time_monotonic {
            self.push_block(HELPER_PY_TIME_MONOTONIC);
        }
        if self.uses.py_time_monotonic_ns {
            self.push_block(HELPER_PY_TIME_MONOTONIC_NS);
        }
        if self.uses.py_time_perf_counter {
            self.push_block(HELPER_PY_TIME_PERF_COUNTER);
        }
        if self.uses.py_time_perf_counter_ns {
            self.push_block(HELPER_PY_TIME_PERF_COUNTER_NS);
        }
        if self.uses.py_time_process_time {
            self.push_block(HELPER_PY_TIME_PROCESS_TIME);
        }
        if self.uses.py_time_process_time_ns {
            self.push_block(HELPER_PY_TIME_PROCESS_TIME_NS);
        }
        if self.uses.py_time_sleep {
            self.push_block(HELPER_PY_TIME_SLEEP);
        }
        if uses_time_core {
            self.push_block(HELPER_PY_TIME_CORE);
        }
        if self.uses.py_time_localtime {
            self.push_block(HELPER_PY_TIME_LOCALTIME);
        }
        if self.uses.py_time_gmtime {
            self.push_block(HELPER_PY_TIME_GMTIME);
        }
        if self.uses.py_time_strftime {
            self.push_block(HELPER_PY_TIME_STRFTIME);
        }
        if self.uses.py_time_strptime {
            self.push_block(HELPER_PY_TIME_STRPTIME);
        }
        if uses_re_match_struct {
            self.push_block(HELPER_PY_RE_MATCH_STRUCT);
        }
        if uses_re_core {
            self.push_block(HELPER_PY_RE_CORE);
        }
        if self.uses.py_re_search {
            self.push_block(HELPER_PY_RE_SEARCH);
        }
        if self.uses.py_re_match {
            self.push_block(HELPER_PY_RE_MATCH_FN);
        }
        if self.uses.py_re_sub {
            self.push_block(HELPER_PY_RE_SUB);
        }
        if self.uses.py_re_group {
            self.push_block(HELPER_PY_RE_GROUP);
        }
        if self.uses.py_re_span {
            self.push_block(HELPER_PY_RE_SPAN);
        }
        if uses_json_core {
            self.push_block(HELPER_PY_JSON_CORE);
        }
        if self.uses.py_json_dumps {
            self.push_block(HELPER_PY_JSON_DUMPS);
        }
        if self.uses.py_json_loads {
            self.push_block(HELPER_PY_JSON_LOADS);
        }
        if self.uses.py_json_dump {
            self.push_block(HELPER_PY_JSON_DUMP);
        }
        if self.uses.py_json_load {
            self.push_block(HELPER_PY_JSON_LOAD);
        }
        if self.uses.print
            || self.uses.len
            || self.uses.range
            || self.uses.range2
            || self.uses.range3
            || self.uses.round
            || self.uses.type_name
            || self.uses.py_max
            || self.uses.py_min
            || self.uses.py_parse_int
            || self.uses.py_parse_float
            || self.uses.py_chr
            || self.uses.py_ord
            || self.uses.py_next
            || self.uses.py_dict_get
            || self.uses.py_list_get
            || self.uses.py_str_get
            || self.uses.py_index
            || self.uses.py_ascii
            || self.uses.py_string_methods
            || self.uses.py_str_slice
            || self.uses.py_list_slice_step
            || self.uses.py_str_slice_step
            || self.uses.py_file
            || self.uses.py_os_remove
            || self.uses.py_os_getcwd
            || self.uses.py_os_chdir
            || self.uses.py_os_mkdir
            || self.uses.py_os_listdir
            || self.uses.py_os_rmdir
            || self.uses.py_os_rename
            || self.uses.py_os_replace
            || self.uses.py_os_makedirs
            || self.uses.py_os_getenv
            || self.uses.py_os_environ
            || self.uses.py_os_name
            || self.uses.py_os_path_join
            || self.uses.py_os_path_exists
            || self.uses.py_os_path_is_file
            || self.uses.py_os_path_is_dir
            || self.uses.py_os_path_basename
            || self.uses.py_os_path_dirname
            || self.uses.py_os_path_split
            || self.uses.py_os_path_abspath
            || self.uses.py_sys_argv
            || self.uses.py_sys_intern
            || self.uses.py_subprocess_run
            || self.uses.py_urllib_urlparse
            || self.uses.py_urllib_quote
            || self.uses.py_urllib_unquote
            || self.uses.py_urllib_urljoin
            || self.uses.py_urllib_urlencode
            || self.uses.py_urllib_parse_qs
            || self.uses.py_urllib_parse_geturl
            || self.uses.py_urllib_urlopen
            || self.uses.py_urllib_response_read
            || self.uses.py_urllib_response_getcode
            || self.uses.py_urllib_response_geturl
            || self.uses.py_math_factorial
            || self.uses.py_math_gcd
            || self.uses.py_math_lcm
            || self.uses.py_math_comb
            || self.uses.py_math_perm
            || self.uses.py_time_monotonic
            || self.uses.py_time_monotonic_ns
            || self.uses.py_time_perf_counter
            || self.uses.py_time_perf_counter_ns
            || self.uses.py_time_process_time
            || self.uses.py_time_process_time_ns
            || self.uses.py_time_sleep
            || self.uses.py_time_localtime
            || self.uses.py_time_gmtime
            || self.uses.py_time_strftime
            || self.uses.py_time_strptime
            || self.uses.py_re_search
            || self.uses.py_re_match
            || self.uses.py_re_sub
            || self.uses.py_re_group
            || self.uses.py_re_span
            || self.uses.py_json_dumps
            || self.uses.py_json_loads
            || self.uses.py_json_dump
            || self.uses.py_json_load
        {
            self.push_line("");
        }
    }

    /// Decide whether the PyError enum and trait impls are needed.
    fn needs_py_error(&self) -> bool {
        self.top_level_can_throw
            || self.uses.py_error
            || self.ctx.functions.values().any(|sig| sig.can_throw)
            || self.uses.py_parse_int
            || self.uses.py_parse_float
            || self.uses.py_index
            || self.uses.py_list_get
            || self.uses.py_str_get
            || self.uses.py_dict_get
            || self.uses.py_chr
            || self.uses.py_ord
            || self.uses.py_next
            || self.uses.py_max
            || self.uses.py_min
            || self.uses.py_list_slice_step
            || self.uses.py_str_slice_step
            || self.uses.py_file
            || self.uses.py_os_remove
            || self.uses.py_os_getcwd
            || self.uses.py_os_chdir
            || self.uses.py_os_mkdir
            || self.uses.py_os_listdir
            || self.uses.py_os_rmdir
            || self.uses.py_os_rename
            || self.uses.py_os_replace
            || self.uses.py_os_makedirs
            || self.uses.py_os_path_abspath
            || self.uses.py_subprocess_run
            || self.uses.py_urllib_urlopen
            || self.uses.py_json_dumps
            || self.uses.py_json_loads
            || self.uses.py_json_dump
            || self.uses.py_json_load
            || self.uses.py_math_factorial
            || self.uses.py_math_lcm
            || self.uses.py_math_comb
            || self.uses.py_math_perm
            || self.uses.range3
    }

    /// Emit the PyError enum plus Display/Error implementations.
    fn emit_py_error_enum(&mut self) {
        self.push_block(HELPER_PY_ERROR_ENUM);
        self.push_line("");
    }
}
