import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
  onRecover: () => void;
}

interface State {
  error: Error | null;
}

/**
 * A single unexpected render error (a type mismatch between the Rust
 * backend's real wire format and a hand-maintained frontend type is the
 * concrete case that motivated this - see docs/known-limitations.md) must
 * never blank the entire app with no way back. This boundary contains the
 * crash to the current page, keeps the surrounding Shell (sidebar/nav)
 * alive, and always offers a way to recover - the main window itself is
 * never closed or replaced.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div className="page">
          <div className="error-banner">
            <p>Something went wrong displaying this page.</p>
            <p className="text-tertiary" style={{ marginTop: 6, fontSize: 11 }}>
              {this.state.error.message}
            </p>
            <button
              className="btn btn-primary"
              type="button"
              style={{ marginTop: 12 }}
              onClick={() => {
                this.setState({ error: null });
                this.props.onRecover();
              }}
            >
              Back to Overview
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}
