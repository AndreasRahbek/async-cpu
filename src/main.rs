use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::task;

const SIZE: usize = 1000;
const NUM_TASKS: usize = 8;

#[tokio::main]
async fn main() {
    let a = Arc::new(vec![vec![1.0; SIZE]; SIZE]);
    let b = Arc::new(vec![vec![1.0; SIZE]; SIZE]);
    let result = Arc::new(Mutex::new(vec![vec![0.0; SIZE]; SIZE]));

    let start = Instant::now();

    let mut handles = vec![];
    let chunk_size = SIZE / NUM_TASKS;

    for task_id in 0..NUM_TASKS {
        let a = Arc::clone(&a);
        let b = Arc::clone(&b);
        let result = Arc::clone(&result);

        let handle = task::spawn_blocking(move || {
            let start_row = task_id * chunk_size;
            let end_row = if task_id == NUM_TASKS - 1 {
                SIZE
            } else {
                (task_id + 1) * chunk_size
            };

            let mut local_result = vec![vec![0.0; SIZE]; SIZE];

            for i in start_row..end_row {
                for j in 0..SIZE {
                    for k in 0..SIZE {
                        local_result[i][j] += a[i][k] * b[k][j];
                    }
                }
            }

            let mut result = result.lock().unwrap();
            for i in start_row..end_row {
                for j in 0..SIZE {
                    result[i][j] = local_result[i][j];
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.await.unwrap();
    }

    let duration = start.elapsed();
    println!("Time taken (Tokio async with spawn_blocking): {:?}", duration);
}
