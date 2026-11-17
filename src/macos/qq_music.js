(() => {
    'use strict';
    // A document-start script, restricted again by the native frame/origin
    // checks. It observes standard browser media APIs, never account storage.
    let inheritedPlayerFrame=false;
    if(window.top!==window)try{
        inheritedPlayerFrame=window.top.location.origin==='https://y.qq.com' &&
            (location.origin==='https://y.qq.com' || location.href==='about:blank' || location.href==='about:srcdoc');
    }catch(_){}
    if (location.origin !== 'https://y.qq.com' && !inheritedPlayerFrame) return;
    if(window.top!==window && !inheritedPlayerFrame)return;
    // Same-origin player frames share the top document's one media owner. No
    // account/login frame receives a projection or native message bridge.
    if (window.top !== window) {
        const originalPlay=HTMLMediaElement.prototype.play;
        HTMLMediaElement.prototype.play=function(...args){
            try { return window.top.__qingyuQQMusic.playMedia(this,originalPlay,args); }
            catch (_) { this.pause();return Promise.reject(new Error('QQ music player is unavailable')); }
        };
        document.addEventListener('play',event=>{
            try { window.top.__qingyuQQMusic.observeMedia(event.target); } catch (_) { event.target.pause?.(); }
        },true);
        return;
    }
    const media = [];
    const trackedMedia=new WeakSet();
    let activeMedia=null,mediaIntent=0,audioRequest=0,audioLease=0,pendingAudio=null;
    const audioManaged=window.__qingyuQQAudioManaged===true;
    let audioAllowed=!audioManaged;
    let pausedByUser=false,resumeFloor=0;
    const documentToken = typeof crypto !== 'undefined' && crypto.randomUUID ? crypto.randomUUID() : `${Date.now()}-${Math.random()}`;
    const handlers = new Map();
    const events = ['playing', 'pause', 'ended', 'emptied', 'loadedmetadata',
        'durationchange', 'seeked', 'volumechange', 'error'];
    let pending = 0;
    let lastPost = 0;
    let lastMedia = null;
    const mediaTracks = new WeakMap();
    let lastTrack = '';
    let lastLyricFingerprint = '';
    let waitingForLyrics = false;
    let projectionSequence = 0;
    let playingTransition = null;
    let playingIntent = 0;
    let unsettledClick = null;
    let signedOutMarker = null;
    let signedOutSince = 0;
    const createdAt = Date.now();
    let resumeAttempt = null;
    let domIdentity = '';
    let domTransition = null;
    let contentObserver=null;
    let observedRoots=[];
    const finite = value => Number.isFinite(value) ? value : 0;
    const text = value => typeof value === 'string' ? value.slice(0, 1024) : '';
    const trusted = () => location.origin === 'https://y.qq.com';
    const emit = () => {
        if (pending || !trusted()) return;
        pending = setTimeout(() => {
            pending = 0;
            if (!trusted() || Date.now() - lastPost < 200) return;
            lastPost = Date.now();
            try { window.webkit.messageHandlers.qingyuQQMusic.postMessage(projection()); } catch (_) {}
        }, 100);
    };
    function pauseMedia(element) { try { element.pause(); } catch (_) {} }
    function pauseOthers(element) { for(const other of media)if(other!==element)pauseMedia(other); }
    function track(element) {
        if (!element || typeof element.pause!=='function' || typeof element.paused!=='boolean') return;
        if (media.includes(element)) return;
        if (media.length === 8) {
            const old = media.shift();
            pauseMedia(old);
        }
        media.push(element);
        // Keep the small guard on evicted/detached elements too. A late play
        // event from an old source must not escape the observation limit.
        if(trackedMedia.has(element))return;
        trackedMedia.add(element);
        events.forEach(event => element.addEventListener(event, emit));
        for(const event of ['play','playing'])element.addEventListener(event,()=>{
            if(!audioAllowed || pausedByUser || (activeMedia && activeMedia!==element))pauseMedia(element);
            else {activeMedia=element;pauseOthers(element);}
        });
    }
    // QQ may use a detached Audio instance. Wrapping the public play method
    // lets it participate without consulting private player objects or URLs.
    const originalPlay = HTMLMediaElement.prototype.play;
    function cancelPendingAudio() {
        if(!pendingAudio)return;
        const old=pendingAudio;pendingAudio=null;old.reject(new Error('QQ music playback was superseded'));
    }
    function startMedia(element,play,args,ticket) {
        if(!audioAllowed || pausedByUser || ticket!==mediaIntent)return Promise.reject(new Error('QQ music playback is not permitted'));
        activeMedia=element;pauseOthers(element);
        let result;
        try { result=Reflect.apply(play,element,args); } catch(error) { return Promise.reject(error); }
        emit();
        return Promise.resolve(result).then(value=>{
            if(!audioAllowed || pausedByUser || ticket!==mediaIntent){
                if(!audioAllowed || pausedByUser || activeMedia!==element)pauseMedia(element);
                throw new Error('QQ music playback was superseded');
            }
            return value;
        });
    }
    function playMedia(element,play=originalPlay,args=[]) {
        track(element);
        if(pendingAudio?.element===element)return pendingAudio.promise;
        cancelPendingAudio();
        const ticket=++mediaIntent;
        if(audioAllowed && !pausedByUser)return startMedia(element,play,args,ticket);
        pauseMedia(element);
        const request=++audioRequest;
        const pending={element,play,args,ticket,request};
        pending.promise=new Promise((resolve,reject)=>{pending.resolve=resolve;pending.reject=reject;});
        pendingAudio=pending;
        try { window.webkit.messageHandlers.qingyuQQMusic.postMessage({action:'claimAudio',document:documentToken,request,user:navigator.userActivation?.isActive===true}); }
        catch (_) { cancelPendingAudio(); }
        return pending.promise;
    }
    function observeMedia(element) {
        track(element);
        if(!audioAllowed || pausedByUser || (activeMedia && activeMedia!==element))pauseMedia(element);
        else {activeMedia=element;pauseOthers(element);}
    }
    HTMLMediaElement.prototype.play = function (...args) {
        return playMedia(this,originalPlay,args);
    };
    document.addEventListener('play',event=>observeMedia(event.target),true);
    function setAudioLease(lease,allowed,request=0) {
        if(!trusted() || !Number.isSafeInteger(lease) || lease<audioLease)return false;
        if(allowed && (!pendingAudio || pendingAudio.request!==request))return false;
        audioLease=lease;audioAllowed=allowed===true;
        if(!audioAllowed){++mediaIntent;cancelPendingAudio();media.forEach(pauseMedia);return true;}
        pausedByUser=false;
        const pending=pendingAudio;pendingAudio=null;
        if(pending.element)startMedia(pending.element,pending.play,pending.args,pending.ticket).then(pending.resolve,pending.reject);
        else pending.resolve();
        return true;
    }
    function requestNativeAudio() {
        // Web Audio / an official embedded player may expose only public DOM
        // metadata to this document. Native calls this only after WebKit has
        // confirmed a suspended media attempt in the selected player view.
        if(audioAllowed || pausedByUser || pendingAudio || currentMedia() || !playerDOM()?.playing)return false;
        const pending={element:null,ticket:++mediaIntent,request:++audioRequest};
        pending.promise=new Promise((resolve,reject)=>{pending.resolve=resolve;pending.reject=reject;});
        pending.promise.catch(()=>{});pendingAudio=pending;
        try {window.webkit.messageHandlers.qingyuQQMusic.postMessage({action:'claimAudio',document:documentToken,request:pending.request,user:false});}
        catch(_){cancelPendingAudio();return false;}
        return true;
    }
    function cancelResume(request) {
        if(Number.isSafeInteger(request))resumeFloor=Math.max(resumeFloor,request);
        if(resumeAttempt && resumeAttempt.request<=resumeFloor)resumeAttempt=null;
        ++playingIntent;
    }
    try {
        const session = navigator.mediaSession;
        if (session) {
            const originalSet = session.setActionHandler.bind(session);
            session.setActionHandler = function (action, handler) {
                const result = originalSet(action, handler);
                if (typeof handler === 'function') handlers.set(action, handler);
                else handlers.delete(action);
                return result;
            };
        }
    } catch (_) {}
    function currentMedia() {
        Array.from(document.querySelectorAll('audio,video')).slice(0, 8).forEach(track);
        const playing = media.filter(item => !item.paused && !item.ended && !item.error && item.readyState > 0);
        if (playing.length) lastMedia = playing[playing.length - 1];
        else if (!lastMedia || !media.includes(lastMedia) || lastMedia.readyState === 0)
            lastMedia = [...media].reverse().find(item => item.readyState > 0) || media[media.length - 1] || null;
        return lastMedia;
    }
    function snapshot() {
        if (!trusted()) return null;
        const element = currentMedia();
        const session = navigator.mediaSession;
        const metadata = session && session.metadata;
        const dom = playerDOM();
        if (!element && !dom) return null;
        // Detached/preloaded elements can outlive their song. An old paused
        // element must not mask a new DOM-only player or supply its old clock.
        let currentElement=element;
        if(element && dom){
            const previous=mediaTracks.get(element);
            if(previous && previous!==dom.track_key && element.paused)currentElement=null;
            else mediaTracks.set(element,dom.track_key);
        }
        const artwork = metadata && Array.isArray(metadata.artwork) ? metadata.artwork.slice(0, 4) : [];
        const cover = artwork.find(item => item && typeof item.src === 'string' && item.src.length <= 2048);
        return { origin: location.origin, title: dom ? dom.title : text(metadata?.title), artist: dom ? dom.artist : text(metadata?.artist),
            album: dom ? dom.album : text(metadata?.album), artwork: dom ? dom.artwork : (cover ? cover.src : ''),
            track_key: dom?.track_key || [text(metadata?.title),text(metadata?.artist),text(metadata?.album)].join('\u001f'),
            track_url: dom?.track_url || '',
            lyrics_ready: dom?.lyrics_ready ?? true,
            // The website button can lag a pause, failure or track change.
            // Confirm playback with the public media element whenever present.
            playing: audioAllowed && !pausedByUser && (currentElement ? !currentElement.paused && !currentElement.ended && !currentElement.error && currentElement.readyState > 0 : dom?.playing === true),
            position: dom?.position_known ? dom.position : currentElement ? Math.max(0, finite(currentElement.currentTime)) : dom?.position || 0,
            duration: dom?.duration > 0 ? dom.duration : currentElement && Number.isFinite(currentElement.duration) && currentElement.duration > 0 ? currentElement.duration : 0,
            volume: currentElement ? finite(currentElement.volume) : null, muted: currentElement ? !!currentElement.muted : false };
    }
    const playerPage = () => /^\/n\/ryqq(?:_v2)?\/player\/?$/.test(location.pathname);
    const content = node => text((node?.textContent || '').trim());
    function seconds(value) {
        const parts = value.trim().split(':');
        if (parts.length < 2 || parts.length > 3 || parts.some(p => !/^\d{1,3}$/.test(p))) return null;
        return parts.reduce((total, part) => total * 60 + Number(part), 0);
    }
    function publicURL(value) {
        try { const url = new URL(value,location.href); return url.origin === 'https://y.qq.com' && !url.username && !url.password ? url : null; } catch (_) { return null; }
    }
    function songLink(node) {
        if(!node)return null;
        const url=publicURL(node.getAttribute('href')),id=trackID(url?.href);
        if(!id)return null;
        // QQ places subtitles, MV and VIP controls beside the actual link.
        // Compare the official song identity and read only this title node.
        const direct=Array.from(node.childNodes || []).filter(child=>child.nodeType===3).map(child=>child.textContent || '').join('').trim();
        // QQ can retain a previous song in the tooltip while visible text and
        // href already describe the new track. Visible direct text wins.
        const title=text(direct || content(node) || node.getAttribute('title')?.trim());
        return {id,url:url.origin+url.pathname,title};
    }
    function playerDOM() {
        if (!playerPage()) return null;
        const sidebar=songLink(document.querySelector('.song_info__name a'));
        const active=songLink(document.querySelector('.songlist__item--playing .songlist__songname_txt a'));
        const transport=songLink(document.querySelector('.player_music__info a'));
        const candidates=[active,transport,sidebar].filter(Boolean);
        const identities=new Set(candidates.map(song=>song.id));
        let song;
        if(identities.size===1){song=candidates[0];domTransition=null;}
        else if(domTransition && identities.has(domTransition.to) && identities.has(domTransition.from) && identities.size===2){
            song=candidates.find(song=>song.id===domTransition.to);
        }else if(domIdentity && identities.has(domIdentity)){
            // Each public component hydrates separately. Whichever component
            // first moves off the last identity starts a transition; old nodes
            // cannot move it back until the components agree again.
            song=candidates.find(song=>song.id!==domIdentity);
            if(song)domTransition={from:domIdentity,to:song.id};
        }else{
            song=candidates.find(candidate=>candidates.filter(song=>song.id===candidate.id).length>=2) || candidates[0];
        }
        if(!song || !song.title)return null;
        domIdentity=song.id;
        const sidebarMatches=sidebar?.id===song.id;
        const activeMatches=active?.id===song.id;
        const artistSelector=sidebarMatches?'.song_info__singer a.playlist__author':'.songlist__item--playing .songlist__artist a.playlist__author';
        const artist=(sidebarMatches || activeMatches)?Array.from(document.querySelectorAll(artistSelector)).slice(0,12).map(content).filter(Boolean).join(' / '):'';
        const album=sidebarMatches?content(document.querySelector('.song_info__album a')):'';
        const times = content(document.querySelector('.player_music__time')).split('/');
        const timelineMatches=transport?transport.id===song.id:sidebarMatches;
        const position = timelineMatches && times.length === 2 ? seconds(times[0]) : null;
        const duration = timelineMatches && times.length === 2 ? seconds(times[1]) : activeMatches?seconds(content(document.querySelector('.songlist__item--playing .songlist__time'))):null;
        const play = document.querySelector('a.btn_big_play');
        const playing = !!play && (play.classList.contains('btn_big_play--pause') || content(play) === '暂停');
        const image = sidebarMatches?document.querySelector('img.song_info__pic'):null;
        let artwork = image?.currentSrc || image?.getAttribute('src') || '';
        if (artwork.startsWith('//')) artwork = `https:${artwork}`;
        if (!/^https:\/\//.test(artwork)) artwork = '';
        return { title:song.title,artist,album,playing,position:position ?? 0,position_known:position!==null,duration:duration ?? 0,artwork,
            track_url:song.url,track_key:`qq:${song.id}`,lyrics_ready:sidebarMatches };
    }
    function lyricDOM(sample) {
        if (!sample || !playerPage()) return null;
        const nodes = Array.from(document.querySelectorAll('#qrc_ctn p')).slice(0,256);
        const lines = nodes.map(content);
        const fingerprint = lines.join('\n');
        if (lastTrack && lastTrack !== sample.track_key) waitingForLyrics = fingerprint === lastLyricFingerprint;
        else if (waitingForLyrics && fingerprint !== lastLyricFingerprint) waitingForLyrics = false;
        lastTrack = sample.track_key;
        if (sample.lyrics_ready===false || waitingForLyrics || !fingerprint || fingerprint.length > 24000) return null;
        lastLyricFingerprint = fingerprint;
        const current = nodes.findIndex(node => node.classList.contains('on'));
        return { title:sample.title,track_key:sample.track_key,lines,current:current >= 0 ? current : null };
    }
    function libraryRoutes() {
        if (!trusted() || !/^\/n\/ryqq(?:_v2)?\/profile(?:\/|$)/.test(location.pathname)) return [];
        const nodes = [document.querySelector('#like_tab'),document.querySelector('#create_tab'),
            ...Array.from(document.querySelectorAll('a.mod_tab__item')).filter(a => /\/profile\/like\/playlist$/.test(publicURL(a.getAttribute('href'))?.pathname || ''))];
        return [...new Set(nodes.filter(Boolean).map(node => publicURL(node.getAttribute('href'))).filter(url => url &&
            /^\/n\/ryqq(?:_v2)?\/profile\/(?:like\/song|like\/playlist|create)$/.test(url.pathname)).map(url => url.origin+url.pathname))].slice(0,3);
    }
    function authenticationState(logged_in) {
        // Empty/slow/offline documents are not a sign-out signal. Require an
        // explicit, visible profile sign-in action after the page has settled.
        let auth=logged_in?'connected':'restoring';
        if(logged_in){signedOutMarker=null;signedOutSince=0;}
        else if(navigator.onLine!==false && document.readyState==='complete'){
            const marker=Array.from(document.querySelectorAll('a.mod_btn_green,button,iframe')).slice(0,80).find(node=>{
                let officialLogin=false;
                if(node.tagName==='IFRAME'){
                    try{const url=new URL(node.getAttribute('src'),location.href);officialLogin=url.protocol==='https:' &&
                        ['graph.qq.com','open.weixin.qq.com','ssl.ptlogin2.qq.com'].includes(url.hostname);}catch(_){}
                }
                if(!officialLogin && !/^(立即登录|登录QQ音乐|登录 QQ 音乐)$/.test(content(node)))return false;
                const rect=node.getBoundingClientRect?.();
                return !!rect && rect.width>0 && rect.height>0 && getComputedStyle(node).visibility!=='hidden';
            });
            if(marker){if(marker!==signedOutMarker){signedOutMarker=marker;signedOutSince=Date.now();}
                if(Date.now()-signedOutSince>=1500)auth='expired';
            }else{signedOutMarker=null;signedOutSince=0;auth='unknown';}
        }
        return auth;
    }
    function libraryDOM() {
        if (!/^\/n\/ryqq(?:_v2)?\/profile(?:\/|$)/.test(location.pathname)) return null;
        const section = /\/profile\/create$/.test(location.pathname) ? 'created' : /\/profile\/like\/playlist$/.test(location.pathname) ? 'collected' : 'favorites';
        const logged_in = !!document.querySelector('#create_tab') && !!document.querySelector('#like_tab');
        const auth=authenticationState(logged_in);
        const playlists = new Map();
        if (logged_in) {
            const favorites = libraryRoutes().find(href => /\/profile\/like\/song$/.test(href));
            if (favorites) playlists.set('favorites',{id:'favorites',title:'我喜欢',url:favorites});
            if (section !== 'favorites') {
                for (const node of Array.from(document.querySelectorAll('a[href*="/playlist/"]')).slice(0,300)) {
                    const url=publicURL(node.getAttribute('href'));
                    const match=url?.pathname.match(/^\/n\/ryqq(?:_v2)?\/playlist\/(\d{1,20})$/);
                    if (!match) continue;
                    const title=content(node) || text(node.getAttribute('title')) || text(node.querySelector('img')?.getAttribute('alt'));
                    if (title && title.length<=160) playlists.set(match[1],{id:match[1],title,url:url.origin+url.pathname});
                    if(playlists.size>=100)break;
                }
            }
        }
        return { logged_in,auth,loading:auth==='restoring'||auth==='unknown',section,playlists:[...playlists.values()],
            message:logged_in?'':auth==='expired'?'QQ 音乐官方页面要求重新登录':'正在确认 QQ 音乐连接…' };
    }
    function projection() {
        if (!trusted()) return null;
        observePlayerContent();
        const sample=snapshot();
        return {origin:location.origin,document:documentToken,sequence:++projectionSequence,media:sample,library:libraryDOM(),lyrics:lyricDOM(sample)};
    }
    function observePlayerContent() {
        if(typeof MutationObserver!=='function' || !playerPage())return;
        const roots=['.song_info__info','.player_music__info','.player_music__time','#qrc_ctn','.songlist__item--playing','a.btn_big_play']
            .map(selector=>document.querySelector(selector)).filter(Boolean);
        if(roots.length===observedRoots.length && roots.every((root,i)=>root===observedRoots[i]))return;
        contentObserver?.disconnect();observedRoots=roots;
        if(!roots.length){contentObserver=null;return;}
        contentObserver=new MutationObserver(()=>{
            if(!trusted())return;
            // Metadata/lyric changes get a bounded event update even when a
            // hidden page's interval is throttled. Never create a second timer.
            if(Date.now()-lastPost>=200)publishNow();else emit();
        });
        for(const root of roots)contentObserver.observe(root,{subtree:true,childList:true,characterData:true,attributes:true,attributeFilter:['class','href','title','src']});
    }
    function publishNow() {
        if (pending) clearTimeout(pending);
        pending = 0; lastPost = Date.now();
        const value = projection();
        try { window.webkit.messageHandlers.qingyuQQMusic.postMessage(value); } catch (_) {}
        return value;
    }
    function waitForPlayback(desired, key, deadline) {
        return new Promise(resolve => {
            let observer = null, timer = 0, settled = false;
            const finish = success => {
                if (settled) return;
                settled = true; if (timer) clearTimeout(timer); observer?.disconnect();
                resolve(success);
            };
            const check = () => {
                if (timer) clearTimeout(timer);
                const media = snapshot();
                if (!trusted() || !media || media.track_key !== key || Date.now() > deadline) { finish(false); return; }
                if (media.playing === desired) { finish(true); return; }
                timer = setTimeout(check, Math.min(20, Math.max(1, deadline-Date.now()+1)));
            };
            const button = playerPage() ? document.querySelector('a.btn_big_play') : null;
            if (button && typeof MutationObserver === 'function') {
                observer = new MutationObserver(check);
                observer.observe(button.parentElement || button, {subtree:true,attributes:true,attributeFilter:['class'],childList:true,characterData:true});
            }
            check();
        });
    }
    async function setPlaying(request) {
        const valid = () => trusted() && request.document === documentToken && typeof request.track_key === 'string'
            && Number.isFinite(request.deadline) && Date.now() <= request.deadline;
        if (!valid() || (request.value !== 0 && request.value !== 1)) return false;
        const desired = request.value === 1;
        if(desired){pausedByUser=false;if(!audioManaged)audioAllowed=true;}
        const intent = ++playingIntent;
        if (playingTransition) await playingTransition;
        if (!valid() || intent !== playingIntent) return false;
        let before = snapshot();
        if (!before || before.track_key !== request.track_key) return false;
        if (unsettledClick && unsettledClick.key === before.track_key) {
            if (before.playing !== unsettledClick.desired && Date.now() < unsettledClick.until) {
                const previousConfirmed = await waitForPlayback(unsettledClick.desired,before.track_key,Math.min(request.deadline,unsettledClick.until));
                if (!previousConfirmed || !valid() || intent !== playingIntent) return false;
                before = snapshot();
                if (!before || before.track_key !== request.track_key) return false;
            }
            unsettledClick=null;
        }
        const confirm=()=>{if(!desired){pausedByUser=audioManaged;audioAllowed=!audioManaged;++mediaIntent;cancelPendingAudio();media.forEach(pauseMedia);}publishNow();};
        if (before.playing === desired) { confirm(); return true; }
        const run = async () => {
            const element = currentMedia();
            if (playerPage() && document.querySelector('a.btn_big_play')) {
                unsettledClick={key:before.track_key,desired,until:Date.now()+850};
                if (!click('a.btn_big_play')) return false;
            } else if (handlers.has(desired ? 'play' : 'pause')) {
                const handler=handlers.get(desired ? 'play' : 'pause');
                try { Promise.resolve(handler({action:desired?'play':'pause'})).catch(()=>{}); } catch (_) { return false; }
            } else if (element) {
                try { if (desired) Promise.resolve(element.play()).catch(()=>{}); else element.pause(); } catch (_) { return false; }
            } else return false;
            const confirmed = await waitForPlayback(desired,request.track_key,Math.min(request.deadline,Date.now()+450));
            if (confirmed) unsettledClick=null;
            if (confirmed && valid() && intent===playingIntent) confirm();
            return confirmed && valid() && intent===playingIntent;
        };
        const transition=run(); playingTransition=transition;
        try { return await transition; } finally { if (playingTransition === transition) playingTransition=null; }
    }
    function activatePlaylist(target) {
        if (!trusted()) return false;
        const url=publicURL(target);
        if(!url || url.pathname !== location.pathname || !/^\/n\/ryqq(?:_v2)?\/(?:playlist\/\d{1,20}|profile\/like\/song)$/.test(url.pathname))return false;
        const button=Array.from(document.querySelectorAll('a.mod_btn_green')).find(node=>content(node)==='播放全部');
        if(!button || !button.isConnected)return false;
        button.click();return true;
    }
    function trackID(value) {
        const url=publicURL(value);return url && !url.search && !url.hash ? url.pathname.match(/^\/n\/ryqq(?:_v2)?\/songDetail\/([a-zA-Z0-9]{1,64})$/)?.[1] || '' : '';
    }
    async function resumeStep(request) {
        if(!trusted() || !request || !Number.isSafeInteger(request.request) || request.request<=0 ||
            request.request<=resumeFloor || !Number.isFinite(request.deadline) || Date.now()>request.deadline)return {status:'stale'};
        if(authenticationState(!!document.querySelector('#create_tab') && !!document.querySelector('#like_tab'))==='expired')return {status:'expired'};
        const target=trackID(request.track_url);
        const playlist=publicURL(request.playlist_url);
        const validPlaylist=playlist && !playlist.search && !playlist.hash &&
            /^\/n\/ryqq(?:_v2)?\/(?:playlist\/\d{1,20}|profile\/like\/song)$/.test(playlist.pathname);
        if(!target && !validPlaylist)return {status:'invalid'};
        if(request.track_url && !target)return {status:'invalid'};
        if(resumeAttempt && request.request<resumeAttempt.request)return {status:'stale'};
        if(!resumeAttempt || resumeAttempt.request!==request.request)resumeAttempt={request:request.request,clicked:false,seeked:false,seek:0};
        const attempt=resumeAttempt;
        const alive=()=>resumeAttempt===attempt && request.request>resumeFloor && trusted() && Date.now()<=request.deadline;
        const sample=snapshot();
        if(playerPage() && sample && ((target && trackID(sample.track_url)===target) || (!target && request.accept_any))){
            if(!sample.playing){
                if(!attempt.clicked){
                    attempt.clicked=true;
                    const confirmed=await setPlaying({document:documentToken,track_key:sample.track_key,value:1,deadline:Math.min(request.deadline,Date.now()+600)});
                    if(!alive())return {status:'stale'};
                    if(!confirmed)return {status:'waiting',projection:projection()};
                }else return {status:'waiting',projection:projection()};
            }
            const current=snapshot();
            if(!current || !current.playing || (target && trackID(current.track_url)!==target))return {status:'waiting',projection:projection()};
            if(Number.isFinite(request.position) && request.position>1 && !request.preserve_position && !attempt.seeked){
                attempt.seeked=true;const element=currentMedia();
                if(element && Number.isFinite(element.duration) && element.duration>0){
                    const accepted=await command({operation:3,value:Math.min(request.position,Math.max(0,element.duration-1)),deadline:request.deadline});
                    if(!alive())return {status:'stale'};
                    attempt.seek=accepted?1:-1;
                }else attempt.seek=-1;
            }
            publishNow();return {status:'playing',seek:attempt.seek,projection:projection()};
        }
        if(attempt.clicked)return {status:'requested',projection:projection()};
        // These public queue controls were observed on the real official player:
        // songname link -> its own row -> list_menu__play. Match the song URL,
        // never a row index or a title shared by another recording.
        if(target){
            const link=Array.from(document.querySelectorAll('.songlist__songname_txt a')).slice(0,1000)
                .find(node=>trackID(node.getAttribute('href'))===target);
            const row=link?.closest('.songlist__item');const play=row?.querySelector('a.list_menu__play');
            if(play && play.isConnected && play.getAttribute('aria-disabled')!=='true'){
                attempt.clicked=true;play.click();return {status:'requested'};
            }
            // Only a matching public song-detail page may use its primary play
            // control. A missing playlist row never falls back to another song.
            if(trackID(location.href)===target){
                const button=Array.from(document.querySelectorAll('a.mod_btn_green,button.mod_btn_green')).slice(0,20)
                    .find(node=>/^(播放|播放歌曲|播放全部)$/.test(content(node)) && node.isConnected);
                if(button){attempt.clicked=true;button.click();return {status:'requested'};}
            }
        }else if(validPlaylist && playlist.pathname===location.pathname && activatePlaylist(playlist.href)){
            attempt.clicked=true;return {status:'requested'};
        }
        return {status:Date.now()-createdAt<1500?'waiting':'missing'};
    }
    function click(selector) {
        // These three legacy player buttons were inspected on the official
        // player page. No inferred site stores or undocumented endpoints.
        if (!/^\/n\/ryqq(?:_v2)?\/player\/?$/.test(location.pathname)) return false;
        const button = document.querySelector(selector);
        if (!button || !button.isConnected || button.getAttribute('aria-disabled') === 'true') return false;
        button.click();
        return true;
    }
    async function invoke(action, details) {
        const handler = handlers.get(action);
        if (!handler) return false;
        try { await handler({ action, ...details }); return true; } catch (_) { return false; }
    }
    async function command(request) {
        if (!trusted() || !request || !Number.isFinite(request.deadline) || Date.now() > request.deadline) return false;
        const alive = () => trusted() && Date.now() <= request.deadline;
        const element = currentMedia();
        const value = request.value;
        let accepted = false;
        switch (request.operation) {
            case 0: {
                const current=snapshot();if(!current)return false;
                return await setPlaying({...request,document:documentToken,track_key:current.track_key,value:current.playing?0:1});
            }
            case 1: accepted = await invoke('nexttrack', {}); if (!alive()) return false; if (!accepted) accepted = click('a.btn_big_next'); break;
            case 2: accepted = await invoke('previoustrack', {}); if (!alive()) return false; if (!accepted) accepted = click('a.btn_big_prev'); break;
            case 3: {
                if (!element || !Number.isFinite(value) || value < 0 || !Number.isFinite(element.duration) || element.duration <= 0) return false;
                const target = Math.min(value, element.duration);
                accepted = await invoke('seekto', { seekTime: target, fastSeek: false });
                if (!alive()) return false;
                if (!accepted) element.currentTime = target;
                // The public element must confirm the requested position;
                // accepting an action handler alone is insufficient for seek.
                accepted = Math.abs(element.currentTime - target) <= 1;
                break;
            }
            case 4:
                if (!element || !Number.isFinite(value) || value < 0 || value > 1) return false;
                element.volume = value; element.muted = false;
                accepted = Math.abs(element.volume - value) < 0.01 && !element.muted;
                break;
            case 5:
                if (!element || (value !== 0 && value !== 1)) return false;
                element.muted = value === 1; accepted = element.muted === (value === 1);
                break;
            case 6: return await setPlaying(request);
            default: return false;
        }
        emit();
        return accepted;
    }
    function diagnostics() {
        if (!trusted()) return null;
        const kind = href => {
            try {
                const url = new URL(href, location.href);
                if (url.origin !== location.origin) return 'other-origin';
                return url.pathname.replace(/[0-9A-Za-z_-]{12,}/g, ':id').replace(/\d+/g, ':id').slice(0, 120);
            } catch (_) { return ''; }
        };
        const shape = (node, includeText = false) => ({ tag: node.tagName,
            class: String(node.className || '').slice(0, 160), id: String(node.id || '').replace(/\d+/g, ':id').slice(0, 100),
            attributes: Array.from(node.attributes || []).slice(0, 16).map(a => a.name),
            hrefKind: node.hasAttribute?.('href') ? kind(node.getAttribute('href')) : '',
            textLength: (node.textContent || '').trim().length,
            ...(includeText ? { text: text((node.textContent || '').trim()).slice(0, 160) } : {}) });
        const branch = (selector, includeText = false) => {
            const root = document.querySelector(selector);
            return root ? [root, ...Array.from(root.querySelectorAll('*')).slice(0, 50)].map(node => shape(node, includeText)) : [];
        };
        const visibleTabs = Array.from(document.querySelectorAll('a,button,[role="tab"]')).slice(0, 600)
            .filter(node => /^(我喜欢|我创建的|我收藏的|我创建的歌单|我收藏的歌单|创建的歌单|收藏的歌单|创建歌单|收藏歌单|歌单|我的歌单|播放全部)$/.test((node.textContent || '').trim()) || String(node.className||'').includes('mod_tab__item'))
            .slice(0, 20).map(node => ({ ...shape(node), label: (node.textContent || '').trim() }));
        const cards = Array.from(document.querySelectorAll('a[href*="/playlist/"]')).slice(0, 40).map(node => ({
            node: shape(node), parents: [node.parentElement, node.parentElement?.parentElement, node.parentElement?.parentElement?.parentElement].filter(Boolean).map(n => shape(n)),
            children: Array.from(node.querySelectorAll('*')).slice(0, 5).map(n => shape(n)) }));
        const element = currentMedia();
        const projected = projection();
        return { page: /\/player\/?$/.test(location.pathname) ? 'player' : /\/profile(?:\/|$)/.test(location.pathname) ? 'profile' : 'other',
            audio: {managed:audioManaged,allowed:audioAllowed,pending:!!pendingAudio,pausedByUser,tracked:media.length,frames:document.querySelectorAll('iframe').length},
            projection: { title:projected.media?.title || '',playing:projected.media?.playing || false,
                position:projected.media?.position || 0,duration:projected.media?.duration || 0,
                lyricLines:projected.lyrics?.lines.length || 0,currentLyric:projected.lyrics?.current ?? null,
                loggedIn:projected.library?.logged_in || false,playlistCount:projected.library?.playlists.length || 0,
                section:projected.library?.section || '' },
            session: { exists: !!navigator.mediaSession, metadata: !!navigator.mediaSession?.metadata,
                title: text(navigator.mediaSession?.metadata?.title), playback: navigator.mediaSession?.playbackState || '' },
            media: media.slice(0, 8).map(m => ({ tag: m.tagName, paused: m.paused, ended: m.ended, ready: m.readyState,
                position: finite(m.currentTime), duration: finite(m.duration), volume: finite(m.volume), muted: m.muted, selected: m === element })),
            info: branch('.song_info__info', true), timeline: branch('.player_music__info', true),
            time: branch('.player_music__time', true), play: branch('a.btn_big_play', true),
            activeRows: branch('.songlist__item--playing', true), currentRows: branch('.songlist__item--current', true),
            lyrics: branch('#qrc_ctn'), tabs: visibleTabs, cards };
    }
    Object.defineProperty(window, '__qingyuQQMusic', { value: Object.freeze({ snapshot, projection, command, diagnostics, libraryRoutes, activatePlaylist, resumeStep,
        playMedia,observeMedia,setAudioLease,cancelResume,requestNativeAudio }), configurable: false, writable: false });
    let heartbeat = setInterval(emit, 1000);
    document.addEventListener('DOMContentLoaded', emit, { once: true });
    window.addEventListener('pageshow', () => { if (!heartbeat) heartbeat = setInterval(emit, 1000); emit(); });
    window.addEventListener('pagehide', () => {
        contentObserver?.disconnect();contentObserver=null;observedRoots=[];
        if (heartbeat) clearInterval(heartbeat);
        heartbeat = 0;
        if (pending) clearTimeout(pending);
        pending = 0;
        // A delayed pagehide message must carry the old document and sequence,
        // so it cannot clear the next document's accepted playback snapshot.
        try { window.webkit.messageHandlers.qingyuQQMusic.postMessage({origin:location.origin,document:documentToken,sequence:++projectionSequence,media:null,library:null,lyrics:null}); } catch (_) {}
    });
})();
