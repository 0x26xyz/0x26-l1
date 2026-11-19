import { AlertTriangle } from "lucide-react";

interface ErrorBannerProps {
  message: string;
}

export function ErrorBanner({ message }: ErrorBannerProps) {
  return (
    <section className="warning-banner" role="alert">
      <AlertTriangle size={16} />
      <span>{message}</span>
    </section>
  );
}
