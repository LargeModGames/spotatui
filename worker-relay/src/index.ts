/**
 * Spotatui Listening Party relay worker.
 * Routes WebSocket connections to Durable Object rooms by code.
 */

import { DurableObject } from "cloudflare:workers";

const WS_PATH = "/ws";
const CODE_CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const RATE_LIMIT_WINDOW_MS = 60 * 60 * 1000; // 1 hour
const MAX_JOIN_ATTEMPTS = 10;
const CODE_LENGTH = 6;
const MAX_MESSAGE_SIZE = 4 * 1024;
const MAX_PARTICIPANTS = 8;
const INACTIVITY_MS = 5 * 60 * 1000;

function generateRoomCode(): string {
  const bytes = new Uint8Array(CODE_LENGTH);
  crypto.getRandomValues(bytes);
  let code = "";
  for (let i = 0; i < CODE_LENGTH; i++) {
    code += CODE_CHARS[bytes[i]! % CODE_CHARS.length];
  }
  return code;
}

function normalizeDisplayName(value: string | null, fallback: string): string {
  const name = (value ?? "").trim().slice(0, 64);
  return name.length > 0 ? name : fallback;
}

export interface Env {
  PARTY_ROOM: DurableObjectNamespace;
  RATE_LIMIT_KV: KVNamespace;
}

interface Session {
  id: string;
  isHost: boolean;
  name: string;
  ip: string;
}

export class PartyRoom extends DurableObject {
  private sessions: Map<WebSocket, Session> = new Map();
  private hostWs: WebSocket | null = null;
  private roomCode: string = "";
  private hostName: string = "Host";
  private roomControlMode: string = "host_only";
  private ipConnections: Map<string, { hostCount: number; guestCount: number }> = new Map();

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    const roomCode = url.searchParams.get("room_code") ?? "";
    const isHost = url.searchParams.get("is_host") === "1";
    const controlMode = url.searchParams.get("control_mode") ?? "host_only";
    const requestedName = normalizeDisplayName(url.searchParams.get("name"), "Guest");

    if (!roomCode) {
      return new Response("Missing room_code", { status: 400 });
    }

    this.roomCode = roomCode;

    const webSocketPair = new WebSocketPair();
    const [client, server] = Object.values(webSocketPair);
    server.accept();

    const sessionId = crypto.randomUUID();

    const clientIp = request.headers.get("CF-Connecting-IP") ?? "unknown";
    const session: Session = {
      id: sessionId,
      isHost,
      name: isHost ? "Host" : requestedName,
      ip: clientIp,
    };

    if (isHost && this.hostWs !== null && this.sessions.has(this.hostWs)) {
      this.sendJson(server, { type: "error", message: "Room already has a host" });
      server.close(1008, "Host exists");
      return new Response(null, { status: 101, webSocket: client });
    }

    if (!isHost && !this.hostWs) {
      this.sendJson(server, { type: "error", message: "Room not found" });
      server.close(1008, "Room not found");
      return new Response(null, { status: 101, webSocket: client });
    }

    if (this.sessions.size >= MAX_PARTICIPANTS) {
      this.sendJson(server, { type: "error", message: "Room is full" });
      server.close(1008, "Room full");
      return new Response(null, { status: 101, webSocket: client });
    }

    const ipEntry = this.ipConnections.get(clientIp) ?? { hostCount: 0, guestCount: 0 };
    if (isHost && ipEntry.hostCount >= 1) {
      this.sendJson(server, { type: "error", message: "Host already connected from this IP" });
      server.close(1008, "Host already connected from this IP");
      return new Response(null, { status: 101, webSocket: client });
    }
    if (!isHost && ipEntry.guestCount >= 2) {
      this.sendJson(server, { type: "error", message: "Too many connections from your IP" });
      server.close(1008, "Too many connections from your IP");
      return new Response(null, { status: 101, webSocket: client });
    }

    this.sessions.set(server, session);
    if (isHost) {
      ipEntry.hostCount++;
      this.hostWs = server;
      // Only the admitted host sets the mode; a rejected one must not change it.
      this.roomControlMode = controlMode === "shared_control" ? "shared_control" : "host_only";
      this.scheduleInactivityAlarm();
    } else {
      ipEntry.guestCount++;
    }
    this.ipConnections.set(clientIp, ipEntry);

    server.addEventListener("message", (event: MessageEvent) => {
      this.handleMessage(server, event.data);
    });
    server.addEventListener("close", () => {
      this.handleClose(server);
    });

    if (isHost) {
      this.sendJson(server, {
        type: "room_created",
        code: roomCode,
        control_mode: this.roomControlMode,
      });
    } else {
      this.sendJson(server, {
        type: "joined_room",
        host_name: this.hostName,
        control_mode: this.roomControlMode,
      });
      this.broadcastExcept(server, { type: "guest_joined", name: session.name });
    }

    return new Response(null, { status: 101, webSocket: client });
  }

  private sendJson(ws: WebSocket, obj: object): void {
    try {
      const raw = JSON.stringify(obj);
      if (raw.length > MAX_MESSAGE_SIZE) return;
      ws.send(raw);
    } catch {
      // ignore
    }
  }

  private handleMessage(ws: WebSocket, data: string | ArrayBuffer): void {
    this.scheduleInactivityAlarm();
    const session = this.sessions.get(ws);
    if (!session) return;

    const raw = typeof data === "string" ? data : new TextDecoder().decode(data);
    if (raw.length > MAX_MESSAGE_SIZE) return;

    let msg: { type: string; [k: string]: unknown };
    try {
      msg = JSON.parse(raw) as { type: string; [k: string]: unknown };
    } catch {
      return;
    }

    switch (msg.type) {
      case "set_name": {
        const name =
          typeof msg.name === "string"
            ? normalizeDisplayName(msg.name, session.isHost ? "Host" : "Guest")
            : session.name;
        const prevName = session.name;
        session.name = name;
        if (!session.isHost && prevName !== session.name) {
          this.broadcastExcept(ws, { type: "guest_left", name: prevName });
          this.broadcastExcept(ws, { type: "guest_joined", name: session.name });
        }
        if (session.isHost) this.hostName = session.name;
        break;
      }
      case "set_control_mode": {
        if (!session.isHost) break;
        this.roomControlMode = msg.control_mode === "shared_control" ? "shared_control" : "host_only";
        this.broadcastAll({ type: "set_control_mode", control_mode: this.roomControlMode });
        break;
      }
      case "sync_state": {
        if (session.isHost) {
          this.broadcastExcept(ws, msg);
        }
        break;
      }
      case "playback_command": {
        if (session.isHost) break;
        if (this.roomControlMode === "shared_control" && this.hostWs) {
          this.sendJson(this.hostWs, { ...msg, from: session.name });
        }
        break;
      }
      case "heartbeat":
        break;
      default:
        break;
    }
  }

  private broadcastExcept(exclude: WebSocket, msg: object): void {
    const raw = JSON.stringify(msg);
    if (raw.length > MAX_MESSAGE_SIZE) return;
    this.sessions.forEach((_, ws) => {
      if (ws !== exclude && ws.readyState === WebSocket.OPEN) {
        ws.send(raw);
      }
    });
  }

  private handleClose(ws: WebSocket): void {
    const session = this.sessions.get(ws);
    this.sessions.delete(ws);
    if (session) {
      const entry = this.ipConnections.get(session.ip);
      if (entry) {
        if (session.isHost) entry.hostCount--;
        else entry.guestCount--;
        if (entry.hostCount <= 0 && entry.guestCount <= 0) {
          this.ipConnections.delete(session.ip);
        } else {
          this.ipConnections.set(session.ip, entry);
        }
      }
    }
    if (ws === this.hostWs) {
      this.hostWs = null;
      this.broadcastAll({ type: "room_closed" });
      this.sessions.forEach((_, s) => {
        try {
          s.close(1000, "Host left");
        } catch {
          // ignore
        }
      });
      this.sessions.clear();
    } else if (session) {
      this.broadcastExcept(ws, { type: "guest_left", name: session.name });
    }
    try {
      ws.close(1000, "Closed");
    } catch {
      // ignore
    }
  }

  private broadcastAll(msg: object): void {
    const raw = JSON.stringify(msg);
    if (raw.length > MAX_MESSAGE_SIZE) return;
    this.sessions.forEach((_, ws) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(raw);
      }
    });
  }

  private scheduleInactivityAlarm(): void {
    this.ctx.storage.setAlarm(Date.now() + INACTIVITY_MS);
  }

  async alarm(): Promise<void> {
    this.broadcastAll({ type: "room_closed" });
    this.sessions.forEach((_, ws) => {
      try {
        ws.close(1000, "Room expired");
      } catch {
        // ignore
      }
    });
    this.sessions.clear();
    this.hostWs = null;
  }
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    if (!url.pathname.endsWith(WS_PATH)) {
      return new Response("Not Found", { status: 404 });
    }

    const upgrade = request.headers.get("Upgrade");
    if (!upgrade || upgrade !== "websocket") {
      return new Response("Expected Upgrade: websocket", { status: 426 });
    }
    if (request.method !== "GET") {
      return new Response("Expected GET", { status: 400 });
    }

    const action = url.searchParams.get("action");
    let roomCode: string;
    let doUrl: URL;

    if (action === "create") {
      roomCode = generateRoomCode();
      const controlMode = url.searchParams.get("control_mode") ?? "host_only";
      doUrl = new URL(request.url);
      doUrl.searchParams.set("room_code", roomCode);
      doUrl.searchParams.set("is_host", "1");
      doUrl.searchParams.set("control_mode", controlMode);
    } else if (action === "join") {
      const code = url.searchParams.get("code");
      const guestName = normalizeDisplayName(url.searchParams.get("name"), "");
      if (!code || code.length !== CODE_LENGTH || !/^[A-Z0-9]+$/i.test(code)) {
        return new Response("Invalid or missing room code", { status: 400 });
      }
      if (!guestName) {
        return new Response("Missing guest name", { status: 400 });
      }
      roomCode = code.toUpperCase();

      const clientIp = request.headers.get("CF-Connecting-IP") ?? "unknown";
      const rateLimitKey = `ratelimit:${clientIp}`;
      const countStr = await env.RATE_LIMIT_KV.get(rateLimitKey);
      const count = countStr !== null ? parseInt(countStr, 10) : 0;
      if (count >= MAX_JOIN_ATTEMPTS) {
        return new Response("Too many attempts", { status: 429 });
      }
      await env.RATE_LIMIT_KV.put(rateLimitKey, String(count + 1), {
        expirationTtl: Math.ceil(RATE_LIMIT_WINDOW_MS / 1000),
      });

      doUrl = new URL(request.url);
      doUrl.searchParams.set("room_code", roomCode);
      doUrl.searchParams.set("is_host", "0");
      doUrl.searchParams.set("name", guestName);
    } else {
      return new Response("Missing action=create or action=join", { status: 400 });
    }

    const id = env.PARTY_ROOM.idFromName(roomCode);
    const stub = env.PARTY_ROOM.get(id);
    return stub.fetch(new Request(doUrl, request));
  },
};
