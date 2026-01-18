import { useEffect, useState } from "react";
import "./app.css";

type HealthResponse = {
  status: string;
  version: string;
};

export function App() {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function run() {
      try {
        const res = await fetch("/api/health");
        if (!res.ok) {
          throw new Error(`HTTP ${res.status}`);
        }
        const json = (await res.json()) as HealthResponse;
        if (!cancelled) {
          setHealth(json);
        }
      } catch (e) {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
        }
      }
    }

    run();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="container">
      <h1>catnap</h1>
      <p className="muted">UI version: {import.meta.env.VITE_APP_VERSION}</p>

      {health ? (
        <pre className="card">{JSON.stringify(health, null, 2)}</pre>
      ) : error ? (
        <p className="error">{error}</p>
      ) : (
        <p>Loading...</p>
      )}
    </div>
  );
}
