declare module "@novnc/novnc" {
  export interface RFBOptions {
    credentials?: Record<string, string>;
    shared?: boolean;
  }

  export interface RFBClipboardEvent extends Event {
    detail: { text: string };
  }

  export interface RFBDisconnectEvent extends Event {
    detail: { clean: boolean };
  }

  export default class RFB extends EventTarget {
    constructor(target: HTMLElement, url: string, options?: RFBOptions);
    background: string;
    clipViewport: boolean;
    focusOnClick: boolean;
    resizeSession: boolean;
    scaleViewport: boolean;
    viewOnly: boolean;
    clipboardPasteFrom(text: string): void;
    disconnect(): void;
  }
}
