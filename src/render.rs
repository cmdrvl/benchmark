use crate::refusal::RefusalEnvelope;

pub fn render_refusal(
    refusal: &RefusalEnvelope,
    json_mode: bool,
) -> Result<String, serde_json::Error> {
    refusal.render(json_mode)
}
