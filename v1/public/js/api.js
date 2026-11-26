async function request(method, url, body) {
  const opts = { method, headers: { 'Content-Type': 'application/json' } };
  if (body) opts.body = JSON.stringify(body);
  const res  = await fetch(url, opts);
  const data = await res.json();
  if (!res.ok) throw new Error(data.error || 'Request failed');
  return data;
}

export const api = {
  get:    (url)        => request('GET',    url),
  post:   (url, body)  => request('POST',   url, body),
  put:    (url, body)  => request('PUT',    url, body),
  delete: (url, body)  => request('DELETE', url, body),
};
