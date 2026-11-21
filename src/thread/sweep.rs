use serde_json::Value;

use crate::net::client::Api;
use crate::support::consts::THREAD_PURGE;
use crate::support::error::{AppError, AppResult};

pub fn purge_path(user_uuid: &str) -> String {
    format!("{}/{}/all", THREAD_PURGE, user_uuid)
}

pub async fn purge(api: &Api, token: &str, user_uuid: &str) -> AppResult<usize> {
    if user_uuid.trim().is_empty() {
        return Err(AppError::api("the thread purge needs the user id"));
    }
    let reply = api.post_empty(&purge_path(user_uuid), token).await?;
    Ok(count(&reply))
}

pub async fn purge_quiet(api: &Api, token: &str, user_uuid: &str) -> Option<usize> {
    match purge(api, token, user_uuid).await {
        Ok(count) => Some(count),
        Err(err) => {
            crate::support::log::lr(&format!(
                "The thread shelf could not be swept, the next turn may continue the old session: {}",
                err
            ));
            None
        }
    }
}

pub fn count(value: &Value) -> usize {
    for key in ["deleted", "Deleted", "count", "threads"] {
        if let Some(number) = value.get(key).and_then(|v| v.as_u64()) {
            return number as usize;
        }
    }
    if let Some(data) = value.get("data") {
        for key in ["deleted", "Deleted", "count", "threads"] {
            if let Some(number) = data.get(key).and_then(|v| v.as_u64()) {
                return number as usize;
            }
        }
        if let Some(number) = from_message(data) {
            return number;
        }
    }
    from_message(value).unwrap_or(0)
}

fn from_message(value: &Value) -> Option<usize> {
    let text = value.get("message").and_then(|v| v.as_str())?;
    let mut words = text.split_whitespace();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("deleted") {
            if let Some(number) = words.next().and_then(|n| n.parse::<usize>().ok()) {
                return Some(number);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd";

    #[test]
    fn the_purge_path_names_the_user_and_all_threads() {
        assert_eq!(
            purge_path(UUID),
            format!("/account/thread/delete/{}/all", UUID)
        );
    }

    #[test]
    fn the_count_is_read_from_the_top_level() {
        let value: Value = serde_json::from_str(r#"{"deleted":5}"#).unwrap();
        assert_eq!(count(&value), 5);
    }

    #[test]
    fn the_count_is_read_from_a_capitalised_key() {
        let value: Value = serde_json::from_str(r#"{"Deleted":3}"#).unwrap();
        assert_eq!(count(&value), 3);
    }

    #[test]
    fn the_count_is_read_from_a_wrapper() {
        let value: Value = serde_json::from_str(r#"{"data":{"deleted":7}}"#).unwrap();
        assert_eq!(count(&value), 7);
    }

    #[test]
    fn the_count_is_read_from_the_message_line() {
        let value: Value = serde_json::from_str(
            r#"{"message":"Deleted 5 threads for user 3f0a1c7e-5b24-4d81-9a6f-2c8e7b4105dd","status":"success","code":null}"#,
        )
        .unwrap();
        assert_eq!(count(&value), 5);
    }

    #[test]
    fn a_reply_without_a_count_reads_zero() {
        let value: Value = serde_json::from_str(r#"{"status":"success"}"#).unwrap();
        assert_eq!(count(&value), 0);
    }
}
