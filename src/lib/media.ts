const PROXY_PREFIX = 'media://localhost/';

export function proxiedMedia(url: string): string {
  return PROXY_PREFIX + btoa(url).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
