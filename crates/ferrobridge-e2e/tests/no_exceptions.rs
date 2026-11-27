//! `tests/fixtures/no_exceptions`: `[library] exceptions = false` sözüyle fonksiyonlar doğrudan
//! değer döndürür; söz bozulursa program sonlanır (tanımsız davranış değil).

use std::process::Command;

use ferrobridge_e2e::no_exceptions::{self as calc, Money};

#[test]
fn functions_return_plain_values() {
    let sum: i32 = calc::add(2, 3);
    assert_eq!(sum, 5);
    // Operatör de `noexcept` yazılmadan trait oldu.
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
        // Alt süreç: C++ exception fırlatır, `noexcept` sarmalayıcı `std::terminate` çağırır.
        let _ = calc::checked_sqrt(-1);
        // Buraya gelinmemeli.
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
        panic!("alt süreç başlatılamadı");
    };
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "süreç sonlanmalıydı: {stderr}");
    assert!(stderr.contains("negatif sayının karekökü"), "{stderr}");
}
