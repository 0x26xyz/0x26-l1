import type { RuntimeEventRecord } from "../lib/chainClient";
import type { StreamStatus } from "../hooks/useEventStream";

interface EventStreamCardProps {
  events: RuntimeEventRecord[];
  status: StreamStatus;
}

function eventTopicLabel(event: RuntimeEventRecord | null): string {
  if (!event) return "No live event yet";
  const [topic] = Object.keys(event.kind);
  return topic ?? "unknown";
}

function formatTimestamp(timestampMs: number | null | undefined): string {
  if (timestampMs == null) return "—";
  return new Date(timestampMs).toLocaleString();
}

function statusLabel(status: StreamStatus): string {
  switch (status) {
    case "open":
      return "Stream connected";
    case "connecting":
      return "Connecting to event stream…";
    case "disconnected":
      return "Stream disconnected";
  }
}

export function EventStreamCard({ events, status }: EventStreamCardProps) {
  const latest = events.at(-1) ?? null;

  return (
    <section className="card">
      <div className="section-title">Live event stream</div>
      <div className="transition-block">
        <strong>{eventTopicLabel(latest)}</strong>
        <span>
          {latest
            ? `seq ${latest.seq}`
            : status === "connecting"
              ? "Opening SSE connection…"
              : "No event received yet"}
        </span>
        <small>
          {latest ? formatTimestamp(latest.timestamp_ms) : statusLabel(status)}
        </small>
      </div>
      <div className={`stream-status stream-status--${status}`}>
        <span className="stream-dot" />
        {statusLabel(status)}
      </div>
      <div className="event-list">
        {events.length ? (
          events
            .slice()
            .reverse()
            .map((event) => (
              <div key={event.seq} className="event-row">
                <div>{eventTopicLabel(event)}</div>
                <small>seq {event.seq}</small>
              </div>
            ))
        ) : (
          <div className="empty-state">
            {status === "connecting"
              ? "Waiting for first event…"
              : "No events yet."}
          </div>
        )}
      </div>
    </section>
  );
}
