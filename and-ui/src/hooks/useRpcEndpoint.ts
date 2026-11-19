import { useCallback, useState } from "react";
import { DEFAULT_RPC_URL, RPC_STORAGE_KEY } from "../config";

function readStored(): string {
  try {
    return window.localStorage.getItem(RPC_STORAGE_KEY) ?? DEFAULT_RPC_URL;
  } catch {
    return DEFAULT_RPC_URL;
  }
}

export function useRpcEndpoint() {
  const [rpcUrl, setRpcUrl] = useState<string>(readStored);

  const commit = useCallback((next: string) => {
    const trimmed = next.trim();
    if (!trimmed) {
      return;
    }
    setRpcUrl(trimmed);
    try {
      window.localStorage.setItem(RPC_STORAGE_KEY, trimmed);
    } catch {
      // localStorage unavailable — keep the in-memory value.
    }
  }, []);

  return { rpcUrl, commit };
}
