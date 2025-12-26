let isAuthenticated = false;

const loginScreen = document.getElementById('login-screen');
const mainScreen = document.getElementById('main-screen');
const loginForm = document.getElementById('login-form');
const loginError = document.getElementById('login-error');
const logoutBtn = document.getElementById('logout-btn');
const searchForm = document.getElementById('search-form');
const clearBtn = document.getElementById('clear-btn');
const mediaList = document.getElementById('media-list');
const resultCount = document.getElementById('result-count');
const mediaDetail = document.getElementById('media-detail');
const closeDetailBtn = document.getElementById('close-detail-btn');

loginForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    const password = document.getElementById('password').value;

    try {
        const response = await fetch('/api/auth/login', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ password }),
        });

        if (response.ok) {
            isAuthenticated = true;
            loginScreen.style.display = 'none';
            mainScreen.style.display = 'block';
            loginError.classList.remove('show');
            loadMedia();
        } else {
            loginError.textContent = 'Invalid password';
            loginError.classList.add('show');
        }
    } catch (error) {
        loginError.textContent = 'Connection error';
        loginError.classList.add('show');
    }
});

logoutBtn.addEventListener('click', async () => {
    await fetch('/api/auth/logout', { method: 'POST' });
    isAuthenticated = false;
    loginScreen.style.display = 'flex';
    mainScreen.style.display = 'none';
    document.getElementById('password').value = '';
});

searchForm.addEventListener('submit', async (e) => {
    e.preventDefault();
    loadMedia();
});

clearBtn.addEventListener('click', () => {
    document.getElementById('search-title').value = '';
    document.getElementById('search-artist').value = '';
    document.getElementById('search-series').value = '';
    document.getElementById('search-media-type').value = '';
    loadMedia();
});

closeDetailBtn.addEventListener('click', () => {
    mediaDetail.style.display = 'none';
});

async function loadMedia() {
    const title = document.getElementById('search-title').value;
    const artist = document.getElementById('search-artist').value;
    const series = document.getElementById('search-series').value;
    const mediaType = document.getElementById('search-media-type').value;

    const params = new URLSearchParams();
    if (title) params.append('title', title);
    if (artist) params.append('artist', artist);
    if (series) params.append('series', series);
    if (mediaType) params.append('media_type', mediaType);

    try {
        const response = await fetch(`/api/media?${params.toString()}`);
        const data = await response.json();

        displayMedia(data.media);
        resultCount.textContent = data.count;
    } catch (error) {
        console.error('Error loading media:', error);
    }
}

function displayMedia(mediaArray) {
    mediaList.innerHTML = '';

    if (mediaArray.length === 0) {
        mediaList.innerHTML = '<p>No results found</p>';
        return;
    }

    mediaArray.forEach((media) => {
        const item = document.createElement('div');
        item.className = 'media-item';
        item.innerHTML = `
            <h3>${escapeHtml(media.title)}</h3>
            <p>Artist: ${escapeHtml(media.artist || 'N/A')}</p>
            <p>Type: ${media.media_type}</p>
            ${media.series ? `<p>Series: ${escapeHtml(media.series)}</p>` : ''}
        `;
        item.addEventListener('click', () => loadMediaDetail(media.id));
        mediaList.appendChild(item);
    });
}

async function loadMediaDetail(id) {
    try {
        const response = await fetch(`/api/media/${id}`);
        const data = await response.json();

        displayMediaDetail(data);
    } catch (error) {
        console.error('Error loading media detail:', error);
    }
}

function displayMediaDetail(data) {
    const { media, tags, attributes } = data;

    document.getElementById('detail-title').textContent = media.title;

    const content = document.getElementById('detail-content');
    content.innerHTML = `
        <dl>
            <dt>ID</dt><dd>${media.id}</dd>
            <dt>Type</dt><dd>${media.media_type}</dd>
            ${media.artist ? `<dt>Artist</dt><dd>${escapeHtml(media.artist)}</dd>` : ''}
            ${media.series ? `<dt>Series</dt><dd>${escapeHtml(media.series)}</dd>` : ''}
            ${media.volume_text ? `<dt>Volume</dt><dd>${escapeHtml(media.volume_text)}</dd>` : ''}
            ${media.page_count ? `<dt>Pages</dt><dd>${media.page_count}</dd>` : ''}
            ${media.path ? `<dt>Path</dt><dd>${escapeHtml(media.path)}</dd>` : ''}
            ${media.description ? `<dt>Description</dt><dd>${escapeHtml(media.description)}</dd>` : ''}
            <dt>Created</dt><dd>${new Date(media.created_at).toLocaleString()}</dd>
            <dt>Updated</dt><dd>${new Date(media.updated_at).toLocaleString()}</dd>
        </dl>

        ${tags.length > 0 ? `
            <h3>Tags</h3>
            <div class="tags">
                ${tags.map(tag => `<span class="tag">${escapeHtml(tag.name)}</span>`).join('')}
            </div>
        ` : ''}

        ${attributes.length > 0 ? `
            <h3>Attributes</h3>
            <dl>
                ${attributes.map(attr => `
                    <dt>${escapeHtml(attr.key)}</dt>
                    <dd>${escapeHtml(attr.value || 'N/A')}</dd>
                `).join('')}
            </dl>
        ` : ''}
    `;

    mediaDetail.style.display = 'flex';
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
