export type Platform = 'Instagram' | 'Twitter' | 'LinkedIn';

export interface Post {
  id: string;
  platform: Platform;
  author_id: string;
  author_username: string;
  content: string;
  media_urls: string[];
  poster_url: string | null;
  liker_ids: string[];
  commenter_ids: string[];
  timestamp: number;
  is_video: boolean;
  engagement_score: number | null;
  is_synthetic: boolean | null;
  author_is_mutual: boolean | null;
  author_is_close_friend: boolean | null;
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

export interface Conversation {
  id: string;
  platform: Platform;
  participants: string[];
  last_message_at: number;
  unread: boolean;
}

export interface SyncResult {
  posts_added: number;
  messages_added: number;
  errors: string[];
}

export interface StoryItem {
  id: string;
  media_type: number;
  media_url: string;
  poster_url: string | null;
  is_video: boolean;
  timestamp: number;
  expiring_at: number;
  caption: string;
}

export interface StoryUser {
  id: string;
  username: string;
  profile_pic_url: string;
  items: StoryItem[];
  is_mutual: boolean;
  is_close_friend: boolean;
}

export interface Comment {
  id: string;
  post_id: string;
  platform: Platform;
  author_id: string;
  author_username: string;
  content: string;
  timestamp: number;
  likes: number;
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
