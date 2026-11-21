use crate::support::error::AppResult;
use crate::support::log::{gap, lg, ly};

pub async fn run() -> AppResult<()> {
    ly("Selftest");

    let challenge = crate::proof::challenge::Challenge {
        challenge_id: "selftest-challenge".to_string(),
        salt: "L-JNabBSXBq0020-RG4s8HuTw3WbM537".to_string(),
        alg: "argon2id".to_string(),
        mem_kib: 19_456,
        iter: 2,
        parallelism: 1,
        hash_len: 32,
        expires_in: 60,
    };
    let solved = crate::proof::solve::solve(&challenge, "selftest-user")?;
    lg(&format!("The proof was solved in {} ms", solved.ms));

    let pieces =
        crate::openai::tool::parse_blocks(r#"<tool>{"name":"ping","arguments":{}}</tool>"#);
    lg(&format!(
        "The tool protocol found {} call(s)",
        crate::openai::tool::calls(&pieces, "selftest").len()
    ));

    gap();
    Ok(())
}
