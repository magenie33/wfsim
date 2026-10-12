// The worker with its default export alone: local workerd refuses a module
// whose named exports are not handlers, and worker/index.js has several.
import worker from "../../../worker/index.js";
export default worker;
