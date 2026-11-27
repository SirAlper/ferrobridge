//! `tests/fixtures/no_exceptions`: with the `[library] exceptions = false` promise, functions return
//! values directly; if the promise is broken, the program ends (not undefined behavior).

use std::process::Command;

use ferrobridge_e2e::no_exceptions::{self as calc, Money};

#[test]
fn functions_return_plain_values() {
    let sum: i32 = calc::add(2, 3);
    assert_eq!(sum, 5);
    // The operator became a trait too, without `noexcept`.
    let total = Money { cents: 150 } + Money { cents: 50 };
    assert_eq!(total.cents, 200);
    assert_eq!(calc::checked_sqrt(81), 9);
}

#[test]
fn ferrobridge_s_own_checks_still_return_result() {
    assert_eq!(calc::total(&[1, 2, 3]).ok(), Some(6));
    assert!(calc::total(&[0; 300]).is_err());
}

const CHILD: &str = "FERROBRIDGE_E2E_BREAK_PROMISE";

#[test]
fn a_broken_promise_terminates_the_process() {
    if std::env::var_os(CHILD).is_some() {
        // The child process: C++ throws, the `noexcept` wrapper calls `std::terminate`.
        let _ = calc::checked_sqrt(-1);
        // Must not get here.
        std::process::exit(0);
    }
    let output = Command::new(std::env::current_exe().unwrap_or_default())
        .args([
            "--exact",
            "a_broken_promise_terminates_the_process",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output();
    let Ok(output) = output else {
        panic!("could not start the child process");
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the process should have ended: {stderr}"
    );
    assert!(
        stderr.contains("square root of a negative number"),
        "{stderr}"
    );
}
