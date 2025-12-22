Below is a concise summary of all HTTP endpoints implemented in `goat/app/main.py`.

- `GET /health`  
  - Purpose: basic health check.  
  - Response model: `HealthCheckResponse` (`status`, `timestamp`, `version`).  
  - Always returns 200 unless internal error.

- `GET /status`  
  - Purpose: current FieryPit status.  
  - Response model: `StatusResponse` (`status`, `current_evaluator`, `available_evaluators`, `timestamp`).  
  - Errors: 500 if `GoatWrangler` not initialized; otherwise may raise the evaluator manager's `tabu` error code/message via `HTTPException`.

- `POST /evaluate`  
  - Purpose: evaluate an Offering using the current evaluator.  
  - Request body: `EvaluationRequest` — JSON object with `data: { ... }`. Example:  
    - `{ "data": { "input": "..." } }`  
  - Response: evaluator result converted via `response.to_dict()`.  
  - Errors: 500 if `GoatWrangler` not initialized; manager errors propagated as HTTP exceptions.

- `GET /evaluators`  
  - Purpose: list all available evaluators and current status.  
  - Response: JSON from `goat_manager.list_available_evaluators()`.  
  - Errors: 500 if `GoatWrangler` not initialized; manager errors propagated.

- `GET /evaluators/{evaluator_name}`  
  - Purpose: get detailed info for a specific evaluator.  
  - Path param: `evaluator_name` (string).  
  - Response: evaluator configuration from `goat_manager.get_evaluator_info(...)`.  
  - Errors: 500 if uninitialized; manager errors propagated.

- `POST /evaluators/set`  
  - Purpose: set the current active evaluator.  
  - Request body: `EvaluatorSetRequest` — `{ "evaluator": "name" }`.  
  - Response: status JSON from `goat_manager.set_evaluator(...)`.  
  - Errors: 500 if uninitialized; manager errors propagated.

- `POST /evaluators/reset`  
  - Purpose: reset to default echo evaluator.  
  - Response: status JSON from `goat_manager.reset_to_echo()`.  
  - Errors: 500 if uninitialized.

- `POST /evaluators/{evaluator_name}/load`  
  - Purpose: load/verify an evaluator is available.  
  - Path param: `evaluator_name`.  
  - Response: status JSON from `goat_manager.load_evaluator(...)`.  
  - Errors: 500 if uninitialized; manager errors propagated.

- `DELETE /evaluators/{evaluator_name}`  
  - Purpose: unload/unregister an evaluator.  
  - Path param: `evaluator_name`.  
  - Response: status JSON from `goat_manager.unload_evaluator(...)`.  
  - Errors: 500 if uninitialized; manager errors propagated.

- `GET /` (root)  
  - Purpose: API metadata and list of endpoints.  
  - Response: JSON with `name`, `version`, `description`, and `endpoints` map.

Notes:
- Many endpoints return a manager-specific error payload by raising `HTTPException` with `response.tabu.code` and `response.tabu.message`.  
- If `GoatWrangler` is not initialized, several endpoints return `HTTP 500` with detail `"GoatWrangler not initialized"`.  
- Request/response logging and CORS middleware are enabled; `startup` loads evaluators from `config/evaluators.yaml` when present.