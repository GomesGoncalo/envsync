use std::result::Result;

#[tarpc::service]
pub trait RpcService {
    async fn join_or_create_doc(ticket: Option<String>) -> Result<String, String>;
    async fn set_env(profile: String, key: String, val: String) -> Result<(), String>;
    async fn get_env(profile: String, key: String) -> Result<Option<String>, String>;
}
