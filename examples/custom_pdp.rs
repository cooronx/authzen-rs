use std::convert::Infallible;

use async_trait::async_trait;
use authzen_rs::prelude::*;

#[derive(Clone)]
struct AdminPdp;

#[async_trait]
impl PolicyDecisionPoint for AdminPdp {
    type Error = Infallible;

    async fn evaluate(&self, request: EvaluationRequest) -> Result<Decision, Self::Error> {
        Ok(Decision::new(
            request.subject().and_then(Subject::id) == Some("admin"),
        ))
    }
}

fn request(subject_id: &str) -> EvaluationRequest {
    EvaluationRequest::new(
        Subject::new("user", subject_id),
        Action::new("read"),
        Resource::new("document", "123"),
    )
}

#[tokio::main]
async fn main() {
    let pdp = AdminPdp;
    for subject_id in ["admin", "alice"] {
        let decision = pdp.evaluate(request(subject_id)).await.unwrap();
        println!("{subject_id} allowed={}", decision.allowed());
    }
}
