use std::env;

use authzen_rs::prelude::*;

#[tokio::main]
async fn main() -> Result<(), AuthZenError> {
    let request = EvaluationRequest::new(
        Subject::new("user", "alice"),
        Action::new("read"),
        Resource::new("document", "123"),
    );

    let Ok(pdp_url) = env::var("AUTHZEN_PDP_URL") else {
        println!(
            "offline request={}",
            serde_json::to_string_pretty(&request).unwrap()
        );
        println!("set AUTHZEN_PDP_URL to send this request to a real HTTPS PDP");
        return Ok(());
    };

    let mut builder = AuthZenClient::builder(pdp_url);
    if let Ok(token) = env::var("AUTHZEN_TOKEN") {
        builder = builder.bearer_token(token);
    }
    let decision = builder.build().await?.evaluate(request).await?;
    println!("live decision allowed={}", decision.allowed());
    Ok(())
}
