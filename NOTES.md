todo: ui loop and data loop should be isolated and use channels

find a timeout option for the UI polling for keystrokes or better yet,
find an interrupt

```
// Pseudocode for illustrative purposes

// Define request and response types
enum DataRequest {
    SpecificData1,
    SpecificData2,
    // other data requests
}

enum DataResponse {
    Data1(DataType1),
    Data2(DataType2),
    // other data responses
}

async fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let (request_tx, request_rx) = async_channel::unbounded();
    let (response_tx, response_rx) = async_channel::unbounded();

    // Data-fetching loop
    tokio::spawn(async move {
        while let Ok(request) = request_rx.recv().await {
            let response = match request {
                DataRequest::SpecificData1 => {
                    // Fetch data 1
                    DataResponse::Data1(fetch_data1().await)
                }
                DataRequest::SpecificData2 => {
                    // Fetch data 2
                    DataResponse::Data2(fetch_data2().await)
                }
                // other cases
            };
            response_tx.send(response).await.unwrap();
        }
    });

    loop {
        // UI sends data requests based on user interaction or other triggers
        if need_data1() {
            request_tx.send(DataRequest::SpecificData1).await.unwrap();
        }

        // UI updates based on received data
        if let Ok(response) = response_rx.try_recv() {
            match response {
                DataResponse::Data1(data) => {
                    // Update UI with data 1
                }
                DataResponse::Data2(data) => {
                    // Update UI with data 2
                }
                // other cases
            }
        }

        // Redraw UI
        terminal.draw(|f| draw_ui(f))?;
    }
}
```
