// Shared shapes for the WebSession in-app scraper. These are pure client-side
// scrapes executed inside a WebView that holds a live, logged-in session for
// the platform's own origin. No APIs, no server.

export type PlatformName = 'Instagram' | 'Twitter' | 'LinkedIn';

export type ScrapeKind = 'feed' | 'news' | 'inbox' | 'profile' | 'stories';

export interface ScrapeItem {
  id: string;
  text: string;
  author?: string;
  authorId?: string;
  ts?: number;
  media: string[];
  isVideo?: boolean;
}

export interface ScrapeMessage {
  id: string;
  conversationId: string;
  text: string;
  from: string;
  isMine?: boolean;
  ts?: number;
}

export interface ScrapeProfile {
  id: string;
  username: string;
  displayName?: string;
  avatar?: string;
  followers?: number;
}

export interface ScrapeEnvelope {
  platform: PlatformName;
  kind: ScrapeKind;
  capturedAt: number;
  items?: ScrapeItem[];
  messages?: ScrapeMessage[];
  profile?: ScrapeProfile;
  cookies?: Record<string, string>;
}