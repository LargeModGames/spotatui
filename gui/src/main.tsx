import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { Connection, takeLaunchCode } from "./connection";
import "./style.css";

const connection = new Connection(
  takeLaunchCode(window.location, window.history),
  (protocol, handlers) => {
    const socket = new WebSocket(`ws://${window.location.host}/ws`, protocol);
    socket.onmessage = (event) => handlers.message(String(event.data));
    socket.onclose = () => handlers.close();
    return socket;
  },
  window.sessionStorage,
  (retry, ms) => {
    window.setTimeout(retry, ms);
  },
);
connection.start();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App connection={connection} />
  </StrictMode>,
);
