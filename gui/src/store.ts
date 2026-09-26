import type { ServerMessage } from "./bindings/ServerMessage";

export type ChannelMessage = Extract<ServerMessage, { rev: number }>;
export type Kind = ChannelMessage["kind"];
export type Channels = Partial<{
  [K in Kind]: Extract<ChannelMessage, { kind: K }>;
}>;

/** Keeps the newest message per channel: one whose `rev` is below the stored one is stale. */
export function applyChannel(
  channels: Channels,
  message: ChannelMessage,
): Channels {
  const stored = channels[message.kind];
  if (stored && message.rev < stored.rev) return channels;
  return { ...channels, [message.kind]: message };
}
