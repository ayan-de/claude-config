"use client";

import { Component, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { Button } from "@/components/ui/button";

interface Props {
  children: ReactNode;
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface State {
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    // No telemetry yet — log to console so a developer running the app can see it.
    console.error("ErrorBoundary caught:", error, info.componentStack);
  }

  private reset = () => this.setState({ error: null });

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    if (this.props.fallback) return this.props.fallback(error, this.reset);
    return (
      <div className="rounded-lg border bg-card/40 px-4 py-6 text-center">
        <AlertTriangle className="mx-auto size-5 text-destructive" />
        <p className="mt-2 text-sm font-medium">Something went wrong</p>
        <p className="mx-auto mt-1 max-w-sm text-[11px] text-muted-foreground">
          {error.message}
        </p>
        <Button
          size="sm"
          variant="outline"
          className="mt-3 cursor-pointer"
          onClick={this.reset}
        >
          Try again
        </Button>
      </div>
    );
  }
}
