const FETCH_TIMEOUT = 5000; // 5 сек — если сервер не ответил, считаем недоступным

export const apiGet = async (path) => {
    const pin = localStorage.getItem('agent_pin') || '';
    const res = await fetch(path, {
        headers: pin ? { Authorization: `Bearer ${pin}` } : {},
        signal: AbortSignal.timeout(FETCH_TIMEOUT),
    });
    if (!res.ok) throw new Error(`GET ${path} → ${res.status}`);
    return res.json();
};

export const apiPost = async (path, body) => {
    const pin = localStorage.getItem('agent_pin') || '';
    const res = await fetch(path, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
            ...(pin ? { Authorization: `Bearer ${pin}` } : {}),
        },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(FETCH_TIMEOUT),
    });
    if (!res.ok) throw new Error(`POST ${path} → ${res.status}`);
    return res.json();
};