use a2a_client::{A2AClient, AgentCardResolver};
use a2a_types::{Message, Part, PartContent, Role};
use tokio_stream::StreamExt;

#[tokio::main]
async fn main() {
    // 1. Resolve the agent card
    let resolver = AgentCardResolver::new();
    let card = resolver
        .resolve("http://127.0.0.1:3000")
        .await
        .expect("Failed to resolve agent card");
    println!("Connected to agent: {}", card.name);
    if let Some(ref desc) = card.description {
        println!("  Description: {desc}");
    }
    if let Some(ref skills) = card.skills {
        for skill in skills {
            println!("  Skill: {} - {:?}", skill.name, skill.description);
        }
    }

    // 2. Create client from agent card
    let client = A2AClient::from_agent_card(&card).expect("Failed to create client");

    // 3. Send a synchronous message
    println!("\n--- Sending synchronous message ---");
    let message = Message {
        message_id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        parts: vec![Part {
            content: PartContent::Text {
                text: "Hello, A2A!".into(),
            },
            metadata: None,
            filename: None,
            media_type: None,
        }],
        context_id: None,
        task_id: None,
        metadata: None,
        extensions: None,
        reference_task_ids: None,
    };

    let event = client
        .send_message(message)
        .await
        .expect("Failed to send message");
    println!("Response: {event:?}");

    // 4. Send a streaming message
    println!("\n--- Sending streaming message ---");
    let message2 = Message {
        message_id: uuid::Uuid::new_v4().to_string(),
        role: Role::User,
        parts: vec![Part {
            content: PartContent::Text {
                text: "Hello again, streaming!".into(),
            },
            metadata: None,
            filename: None,
            media_type: None,
        }],
        context_id: None,
        task_id: None,
        metadata: None,
        extensions: None,
        reference_task_ids: None,
    };

    let mut stream = client
        .send_message_stream(message2)
        .await
        .expect("Failed to start stream");

    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => println!("Stream event: {event:?}"),
            Err(e) => eprintln!("Stream error: {e}"),
        }
    }

    println!("\nDone!");
}
