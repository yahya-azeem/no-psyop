const MEDIA_HOST = 'http://127.0.0.1:8231';

function encodeBase64Url(s: string): string {
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function proxiedMedia(url: string): string {
  return `${MEDIA_HOST}/media/${encodeBase64Url(url)}`;
}
