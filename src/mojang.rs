use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct MojangProfile {
    id: String,
    name: String,
}

pub async fn get_mojang_uuid(username: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let url = format!("https://api.mojang.com/users/profiles/minecraft/{}", username);
    let client = reqwest::Client::new();
    let response = client.get(&url)
        .send()
        .await?;
    
    if response.status().is_success() {
        let profile: MojangProfile = response.json().await?;
        // Format UUID with hyphens
        let mut uuid = profile.id.to_string();
        uuid.insert(8, '-');
        uuid.insert(13, '-');
        uuid.insert(18, '-');
        uuid.insert(23, '-');
        Ok((uuid, profile.name))
    } else {
        Err("Player not found or Mojang API error".into())
    }
}
