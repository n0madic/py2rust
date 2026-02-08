//! Runtime integration tests.
//!
//! All runtime suites are declared in this single registry so test wiring and
//! expected-output policy stay centralized.

mod common;

/// Generate a standard runtime integration test body around `run_py`.
macro_rules! runtime_case {
    ($test_name:ident, $suite_name:literal, $source_file:literal) => {
        #[test]
        fn $test_name() {
            crate::common::run_py(
                $suite_name,
                include_str!(concat!("runtime/", $source_file)),
                None,
            );
        }
    };
    ($test_name:ident, $suite_name:literal, $source_file:literal, $expected:expr) => {
        #[test]
        fn $test_name() {
            crate::common::run_py(
                $suite_name,
                include_str!(concat!("runtime/", $source_file)),
                Some($expected),
            );
        }
    };
}

/// Declare all runtime suites in one place.
macro_rules! runtime_cases {
    (
        $(($name:ident, $suite:literal, $source:literal $(, $expected:expr)?)),* $(,)?
    ) => {
        $(
            runtime_case!($name, $suite, $source $(, $expected)?);
        )*
    };
}

runtime_cases!(
    (
        runtime_assert_comprehensive,
        "assert",
        "assert.py",
        "All assertions passed!"
    ),
    (runtime_builtins_comprehensive, "builtins", "builtins.py"),
    (runtime_classes_comprehensive, "classes", "classes.py"),
    (runtime_union_method_calls, "classes_union", "classes_union.py"),
    (
        runtime_collections_comprehensive,
        "collections",
        "collections.py",
        "All collection tests passed!"
    ),
    (
        runtime_comprehensions,
        "comprehensions",
        "comprehensions.py",
        "All comprehension tests passed!"
    ),
    (
        runtime_control_flow_comprehensive,
        "control_flow",
        "control_flow.py",
        "All control flow tests passed!"
    ),
    (runtime_core_types_comprehensive, "core_types", "core_types.py"),
    (runtime_exceptions_comprehensive, "exceptions", "exceptions.py"),
    (
        runtime_file_io_comprehensive,
        "file_io",
        "file_io.py",
        "All file I/O tests passed!"
    ),
    (runtime_functions_comprehensive, "functions", "functions.py"),
    (runtime_generators_comprehensive, "generators", "generators.py"),
    (
        runtime_global_scoping_comprehensive,
        "global_scoping",
        "global_scoping.py",
        "All global scoping tests passed!"
    ),
    (
        runtime_import_comprehensive,
        "import",
        "import.py",
        "All import tests passed!"
    ),
    (runtime_stdlib_os_comprehensive, "stdlib_os", "stdlib_os.py"),
    (
        runtime_io_comprehensive,
        "io",
        "io.py",
        "42\nhello\ntrue\nfalse\nworld\n30\nmessage from function\n1\n2\n3"
    ),
    (
        runtime_print_comprehensive,
        "print",
        "print.py",
        "42\n-7\n0\n3.14\n1\n0\ntrue\nfalse\nNone\nhello\n\n\n1 2 3\na b c\n1 hello true 3.14\n1 None 2\n1-2-3\na, b, c\n12\nhello world\nline1\nline2\n1-2-3!\n[1, 2, 3]\n['a', 'b']\n[]\n(1, 2, 3)\n(42,)\n(\"hello\", \"world\")\n[[1, 2], [3, 4]]\n{}\n{\"x\": 1}\n{}\n{42}\n[104, 101, 108, 108, 111]\n[]\n30\n200\n0\n1\n2\nHello World"
    ),
    (runtime_stdlib_sys_comprehensive, "stdlib_sys", "stdlib_sys.py"),
    (runtime_stdlib_re_comprehensive, "stdlib_re", "stdlib_re.py"),
    (runtime_stdlib_json_comprehensive, "stdlib_json", "stdlib_json.py"),
    (runtime_stdlib_math_comprehensive, "stdlib_math", "stdlib_math.py"),
    (runtime_stdlib_time_comprehensive, "stdlib_time", "stdlib_time.py"),
    (
        runtime_stdlib_subprocess_comprehensive,
        "stdlib_subprocess",
        "stdlib_subprocess.py"
    ),
    (
        runtime_stdlib_urllib_comprehensive,
        "stdlib_urllib",
        "stdlib_urllib.py"
    ),
    (runtime_iteration_comprehensive, "iteration", "iteration.py"),
    (
        runtime_match_comprehensive,
        "match",
        "match.py",
        "All match tests passed!"
    ),
    (
        runtime_operators_comprehensive,
        "operators",
        "operators.py",
        "All operator tests passed!"
    ),
    (runtime_strings_comprehensive, "strings", "strings.py"),
    (
        runtime_types_system_comprehensive,
        "types_system",
        "types_system.py"
    )
);
