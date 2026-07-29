export type Platform = 'Instagram' | 'Twitter' | 'LinkedIn';

export interface Post {
  id: string;
  platform: Platform;
  author_id: string;
  author_username: string;
  content: string;
  media_urls: string[];
  liker_ids: string[];
  commenter_ids: string[];
  timestamp: number;
  is_video: boolean;
  engagement_score: number | null;
  is_synthetic: boolean | null;
}

export interface Message {
  id: string;
  platform: Platform;
  conversation_id: string;
  sender_id: string;
  content: string;
  timestamp: number;
}

export interface FeedItem {
  post: Post;
  proximity_score: number;
  relevance_score: number;
}

export interface Digest {
  clusters: ContentCluster[];
  generated_at: number;
}

export interface ContentCluster {
  topic: string;
  summary: string;
  items: Post[];
}

export interface PostFilterResult {
  is_synthetic: boolean;
  bait_score: number;
  should_filter: boolean;
}

export interface Intent {
  query: string;
  platforms: Platform[];
  max_results: number;
}
