import { useEffect, useState } from "react";
import type { ChainClient, RuntimeEventRecord } from "../lib/chainClient";
import { EVENT_BUFFER_SIZE } from "../config";

export type StreamStatus = "connecting" | "open" | "disconnected";

export interface EventStreamState {
  events: RuntimeEventRecord[];
  cursor: number | null;
  status: StreamStatus;
}

export function useEventStream(client: ChainClient): EventStreamState {
  const [events, setEvents] = useState<RuntimeEventRecord[]>([]);
  const [cursor, setCursor] = useState<number | null>(null);
  const [status, setStatus] = useState<StreamStatus>("connecting");

  useEffect(() => {
    setEvents([]);
    setCursor(null);
    setStatus("connecting");

    const source = client.openEventStream(
      {
        onEvent: (event) => {
          setStatus("open");
          setEvents((current) =>
            [...current, event].slice(-EVENT_BUFFER_SIZE)
          );
        },
        onCursor: (nextSeq) => {
          setStatus("open");
          setCursor(nextSeq);
        },
        onError: () => setStatus("disconnected"),
      },
      { limit: EVENT_BUFFER_SIZE, follow: true }
    );

    return () => {
      source.close();
    };
  }, [client]);

  return { events, cursor, status };
}
