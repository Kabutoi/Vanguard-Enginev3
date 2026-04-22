use tokio::sync::mpsc;
use tracing::info;

#[derive(Debug)]
pub enum TaskType {
    Rendering,
    Physics,
    Animation,
    AgenticInference,
}

pub struct EngineTask {
    pub id: usize,
    pub task_type: TaskType,
    pub payload: Box<dyn FnOnce() + Send>,
}

pub struct TaskGraphManager {
    sender: mpsc::Sender<EngineTask>,
}

impl TaskGraphManager {
    pub fn new(worker_count: usize) -> Self {
        let (tx, mut rx) = mpsc::channel::<EngineTask>(1024);
        
        // Simulating worker threads with tokio spawn for the custom graph
        for _ in 0..worker_count {
            // In a real custom graph, workers would pull from the queue
            // and respect task dependencies.
        }

        tokio::spawn(async move {
            while let Some(task) = rx.recv().await {
                info!("Processing Task {} [Type: {:?}]", task.id, task.task_type);
                (task.payload)();
            }
        });

        info!("TaskGraphManager initialized with {} workers active.", worker_count);
        
        TaskGraphManager { sender: tx }
    }

    pub fn dispatch(&self, task: EngineTask) {
        let sender = self.sender.clone();
        tokio::spawn(async move {
            let _ = sender.send(task).await;
        });
    }
}
