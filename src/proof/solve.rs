use std::time::Instant;

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;

use crate::proof::challenge::Challenge;
use crate::support::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct Solution {
    pub proof: String,
    pub ms: u64,
}

pub fn solve(challenge: &Challenge, user_uuid: &str) -> AppResult<Solution> {
    let started = Instant::now();

    let mut password = Vec::with_capacity(challenge.challenge_id.len() + user_uuid.len());
    password.extend_from_slice(challenge.challenge_id.as_bytes());
    password.extend_from_slice(user_uuid.as_bytes());

    let salt = decode_salt(&challenge.salt)?;

    let hash_len = if challenge.hash_len == 0 {
        32
    } else {
        challenge.hash_len
    };
    let params = Params::new(
        challenge.mem_kib.max(8),
        challenge.iter.max(1),
        challenge.parallelism.max(1),
        Some(hash_len),
    )
    .map_err(|err| AppError::proof(format!("the argon2 parameters were refused: {}", err)))?;

    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut digest = vec![0u8; hash_len];
    argon
        .hash_password_into(&password, &salt, &mut digest)
        .map_err(|err| AppError::proof(format!("the proof could not be solved: {}", err)))?;

    let proof = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&digest);
    let ms = started.elapsed().as_millis() as u64;

    Ok(Solution { proof, ms })
}

fn decode_salt(raw: &str) -> AppResult<Vec<u8>> {
    let trimmed = raw.trim();
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(trimmed))
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(trimmed))
        .map_err(|_| AppError::proof("the challenge salt is not base64url"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge() -> Challenge {
        Challenge {
            challenge_id: "KB2qhgwIMV325Bw2tARCDw".to_string(),
            salt: "L-JNabBSXBq0020-RG4s8HuTw3WbM537".to_string(),
            alg: "argon2id".to_string(),
            mem_kib: 19_456,
            iter: 2,
            parallelism: 1,
            hash_len: 32,
            expires_in: 60,
        }
    }

    #[test]
    fn the_proof_is_a_base64url_digest_of_the_right_size() {
        let solution = solve(&challenge(), "3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd").unwrap();
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(&solution.proof)
            .unwrap();
        assert_eq!(bytes.len(), 32);
        assert!(!solution.proof.contains('='));
        assert!(!solution.proof.contains('+'));
        assert!(!solution.proof.contains('/'));
    }

    #[test]
    fn the_proof_is_stable_for_the_same_inputs() {
        let first = solve(&challenge(), "user-1").unwrap();
        let second = solve(&challenge(), "user-1").unwrap();
        assert_eq!(first.proof, second.proof);
    }

    #[test]
    fn a_different_uuid_changes_the_proof() {
        let first = solve(&challenge(), "user-1").unwrap();
        let second = solve(&challenge(), "user-2").unwrap();
        assert_ne!(first.proof, second.proof);
    }

    #[test]
    fn a_different_challenge_changes_the_proof() {
        let mut other = challenge();
        other.challenge_id = "OTHERCHALLENGE".to_string();
        assert_ne!(
            solve(&challenge(), "user-1").unwrap().proof,
            solve(&other, "user-1").unwrap().proof
        );
    }

    #[test]
    fn a_salt_that_is_not_base64_is_refused() {
        let mut broken = challenge();
        broken.salt = "!!!not base64!!!".to_string();
        assert!(solve(&broken, "user-1").is_err());
    }

    #[test]
    fn the_solver_reports_a_time() {
        let solution = solve(&challenge(), "user-1").unwrap();
        assert!(solution.ms < 60_000);
    }
}
