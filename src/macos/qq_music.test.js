// Run with: node --test src/macos/qq_music.test.js
const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const source = fs.readFileSync(`${__dirname}/qq_music.js`, 'utf8');

function fixture(origin = 'https://y.qq.com', realTimers = false, clock = null, managed = false) {
    class Media extends EventTarget {
        constructor() { super(); this.paused = true; this.ended = false; this.readyState = 1; this.currentTime = 12; this.duration = 120; this.volume = 0.4; this.muted = false; this.error = null; }
        play() { this.paused = false; return Promise.resolve(); }
        pause() { this.paused = true; }
    }
    const clicks = [];
    const buttons = new Map(['a.btn_big_play', 'a.btn_big_prev', 'a.btn_big_next'].map(selector => [selector,
        { isConnected: true, getAttribute: () => null, click: () => clicks.push(selector) }]));
    const nodes = new Map(), lists = new Map();
    const window = new EventTarget(); window.top = window;
    window.__qingyuQQAudioManaged=managed;
    const messages = []; window.webkit = { messageHandlers: { qingyuQQMusic: { postMessage: value => messages.push(value) } } };
    const session = { metadata: { title: '歌曲', artist: '歌手', album: '专辑', artwork: [] }, setActionHandler() {} };
    const navigator={mediaSession:session,onLine:true};
    const location = { origin, pathname: '/n/ryqq/player', href: `${origin}/n/ryqq/player` };
    const document = new EventTarget(); document.querySelectorAll = selector => lists.get(selector) || []; document.querySelector = selector => nodes.get(selector) || buttons.get(selector);
    const callbacks = new Map(); let nextTimer = 1;
    const observers = new Set();
    class Observer {
        constructor(callback) {this.callback=callback;}
        observe(root,options) {this.content=options?.attributeFilter?.includes('href');observers.add(this);}
        disconnect() {observers.delete(this);}
    }
    vm.runInNewContext(source, { window, location, document, navigator, HTMLMediaElement: Media,
        setTimeout:realTimers?setTimeout:callback => { const timer = nextTimer++; callbacks.set(timer, callback); return timer; },
        clearTimeout:realTimers?clearTimeout:timer => { callbacks.delete(timer); },
        setInterval() { return nextTimer++; }, clearInterval() {}, URL, MutationObserver:Observer,
        Date:clock?class extends Date {static now(){return clock.now;}}:Date,
        getComputedStyle:node=>({visibility:node.hidden?'hidden':'visible'}) });
    return { bridge: window.__qingyuQQMusic, session, location, Media, clicks, window, messages, nodes, lists,buttons,document,navigator,
        notify:()=>{for(const observer of [...observers])observer.callback();},observerCount:()=>[...observers].filter(observer=>!observer.content).length,
        contentObserverCount:()=>[...observers].filter(observer=>observer.content).length };
}
function node(textContent, attributes = {}, classes = []) {
    return { textContent, isConnected:true, getAttribute:key => attributes[key] || null,getBoundingClientRect:()=>({width:100,height:30}),
        classList:{contains:key => classes.includes(key)}, querySelector:() => null };
}
function playerDOM(f, title='歌曲', id='abc123') {
    f.nodes.set('.song_info__name a',node(title,{href:`/n/ryqq_v2/songDetail/${id}`}));
    f.lists.set('.song_info__singer a.playlist__author',[node('歌手')]);
    f.nodes.set('.song_info__album a',node('专辑'));
    f.nodes.set('.songlist__item--playing .songlist__songname_txt',node(title));
    f.nodes.set('.songlist__item--playing .songlist__songname_txt a',node(title,{href:`/n/ryqq/songDetail/${id}`,title}));
    f.nodes.set('.player_music__info a',node(title,{href:`/n/ryqq_v2/songDetail/${id}`,title}));
    f.nodes.set('.player_music__time',node('01:23 / 04:05'));
    f.nodes.set('a.btn_big_play',node('暂停',{},['btn_big_play--pause']));
    f.nodes.set('img.song_info__pic',node('',{src:'//y.gtimg.cn/cover.jpg'}));
}
const request = (operation, value = 0) => ({ operation, value, deadline: Date.now() + 500 });

test('auth frames never receive a media bridge and navigation revokes an existing bridge', async () => {
    assert.equal(fixture('https://graph.qq.com').bridge, undefined);
    assert.equal(fixture('https://y.qq.com.evil.test').bridge, undefined);
    const f = fixture(); const audio = new f.Media(); await audio.play();
    assert.equal(f.bridge.snapshot().title, '歌曲');
    f.location.origin = 'https://graph.qq.com';
    assert.equal(f.bridge.snapshot(), null);
    assert.equal(await f.bridge.command(request(0)), false);
    assert.equal(audio.paused, false);
});
test('detached audio is observed through public APIs without inventing missing metadata', async () => {
    const f = fixture(); const audio = new f.Media();
    assert.equal(f.bridge.snapshot(), null); await audio.play();
    const playing = f.bridge.snapshot();
    assert.equal(playing.playing, true); assert.equal(playing.position, 12); assert.equal(playing.volume, 0.4);
    audio.pause(); assert.equal(f.bridge.snapshot().playing, false);
    f.session.metadata = null;
    assert.equal(f.bridge.snapshot().title, '');
    assert.equal(f.bridge.snapshot().playing, false);
});
test('a newer media element pauses the previous source, including detached elements beyond the observation limit', async () => {
    const f=fixture();const sources=[];
    for(let index=0;index<12;index++){
        const audio=new f.Media();sources.push(audio);await audio.play();
        assert.equal(sources.filter(source=>!source.paused).length,1,'a page may play only its newest source');
    }
    assert.equal(sources.at(-1).paused,false);
    assert.equal(f.bridge.snapshot().playing,true);
});
test('managed sources wait for their exact native lease and stale grants cannot start cancelled audio', async()=>{
    const f=fixture('https://y.qq.com',false,null,true),first=new f.Media(),second=new f.Media();
    const firstPlay=first.play().then(()=>true,()=>false);const firstClaim=f.messages.at(-1);
    assert.equal(first.paused,true,'native suspension has a matching JavaScript gate');
    const secondPlay=second.play();const secondClaim=f.messages.at(-1);
    assert.equal(await firstPlay,false,'a newer source cancels the pending source');
    assert.equal(f.bridge.setAudioLease(1,true,firstClaim.request),false);
    assert.equal(first.paused,true);assert.equal(second.paused,true);
    assert.equal(f.bridge.setAudioLease(2,true,secondClaim.request),true);await secondPlay;
    assert.equal(second.paused,false);assert.equal(first.paused,true);
    assert.equal(f.bridge.setAudioLease(3,false),true);assert.equal(second.paused,true);
    assert.equal(f.bridge.setAudioLease(2,true,secondClaim.request),false,'revocation rejects a delayed old grant');
    const late=second.play().then(()=>true,()=>false);
    assert.equal(second.paused,true,'a late callback must obtain a new native lease');
    f.bridge.setAudioLease(4,false);assert.equal(await late,false);
});
test('superseded play promises cannot resurrect an earlier source after their late completion',async()=>{
    const f=fixture();const old=new f.Media(),current=new f.Media();let complete;
    const original=()=>{old.paused=false;return new Promise(resolve=>{complete=resolve;});};
    const late=f.bridge.playMedia(old,original,[]).then(()=>true,()=>false);
    await current.play();complete();assert.equal(await late,false);
    assert.equal(old.paused,true);assert.equal(current.paused,false);
});
test('native media fallback requires public player state and keeps revocation effective',async()=>{
    const f=fixture('https://y.qq.com',false,null,true);playerDOM(f);
    assert.equal(f.bridge.projection().media.playing,false,'a suspended DOM button cannot advertise audible playback');
    assert.equal(f.bridge.requestNativeAudio(),true);
    const claim=f.messages.at(-1);assert.equal(claim.action,'claimAudio');
    assert.equal(f.bridge.setAudioLease(1,true,claim.request),true);
    assert.equal(f.bridge.projection().media.playing,true);
    f.bridge.setAudioLease(2,false);assert.equal(f.bridge.projection().media.playing,false);
    f.location.pathname='/n/ryqq_v2/profile';assert.equal(f.bridge.requestNativeAudio(),false,'an account/library page never becomes a fallback audio candidate');
});
test('actual audio state takes precedence over a stale QQ playback button', async () => {
    const f=fixture('https://y.qq.com',true);playerDOM(f);
    const audio=new f.Media();await audio.play();audio.pause();
    assert.equal(f.bridge.snapshot().playing,false,'a pause must not remain playing while the DOM catches up');
    assert.equal(await f.bridge.command(absolute(f,false)),true,'confirmed pause is an immediate no-op');
    assert.equal(f.clicks.length,0);
    await audio.play();playbackButton(f,false,()=>{throw new Error('already playing');});
    assert.equal(f.bridge.snapshot().playing,true,'a stale play label must not hide live audio');
    assert.equal(await f.bridge.command(absolute(f,true)),true);
    playbackButton(f,true,()=>{throw new Error('stale button');});
    audio.ended=true;assert.equal(f.bridge.snapshot().playing,false);
    audio.ended=false;audio.error={code:2};assert.equal(f.bridge.snapshot().playing,false);
    audio.error=null;audio.readyState=0;assert.equal(f.bridge.snapshot().playing,false);
});
test('public session handlers and inspected player buttons control only this page', async () => {
    const f = fixture(); let next = 0;
    f.session.setActionHandler('nexttrack', () => { next++; });
    assert.equal(await f.bridge.command(request(1)), true); assert.equal(next, 1); assert.equal(f.clicks.length, 0);
    f.session.setActionHandler('nexttrack', null);
    assert.equal(await f.bridge.command(request(1)), true); assert.deepEqual(f.clicks, ['a.btn_big_next']);
    f.location.pathname = '/n/ryqq_v2/player';
    assert.equal(await f.bridge.command(request(2)), true); assert.deepEqual(f.clicks, ['a.btn_big_next', 'a.btn_big_prev']);
    f.location.pathname = '/n/ryqq_v2/profile';
    assert.equal(await f.bridge.command(request(2)), false); assert.equal(f.clicks.length, 2);
});
test('seek, volume and mute confirm element state; expired or invalid commands do nothing', async () => {
    const f = fixture(); const audio = new f.Media(); await audio.play();
    assert.equal(await f.bridge.command(request(3, 500)), true); assert.equal(audio.currentTime, 120);
    assert.equal(await f.bridge.command(request(3, -1)), false); assert.equal(audio.currentTime, 120);
    assert.equal(await f.bridge.command(request(5, 1)), true); assert.equal(audio.muted, true); assert.equal(audio.volume, 0.4);
    assert.equal(await f.bridge.command(request(5, 0)), true); assert.equal(audio.volume, 0.4);
    assert.equal(await f.bridge.command(request(4, 0.7)), true); assert.equal(audio.volume, 0.7);
    assert.equal(await f.bridge.command({ ...request(4, 0.1), deadline: Date.now() - 1 }), false); assert.equal(audio.volume, 0.7);
    assert.equal(await f.bridge.command(request(4, Infinity)), false); assert.equal(audio.volume, 0.7);
});
test('observed QQ DOM projects playback and current lyrics without MediaSession metadata or an audio element', () => {
    const f=fixture();f.session.metadata=null;playerDOM(f);
    f.lists.set('#qrc_ctn p',[node('第一行'),node('当前行',{},['on'])]);
    const projected=f.bridge.projection();
    assert.equal(projected.media.title,'歌曲');assert.equal(projected.media.artist,'歌手');assert.equal(projected.media.album,'专辑');
    assert.equal(projected.media.playing,true);assert.equal(projected.media.position,83);assert.equal(projected.media.duration,245);
    assert.equal(projected.media.artwork,'https://y.gtimg.cn/cover.jpg');assert.equal(projected.media.volume,null);
    assert.deepEqual(Array.from(projected.lyrics.lines),['第一行','当前行']);assert.equal(projected.lyrics.current,1);
    f.nodes.set('a.btn_big_play',node('播放'));assert.equal(f.bridge.projection().media.playing,false);
    playerDOM(f,'下一首','next123');assert.equal(f.bridge.projection().lyrics,null,'old lyric text cannot attach to the next track');
    f.lists.set('#qrc_ctn p',[node('新歌词',{},['on'])]);assert.equal(f.bridge.projection().lyrics.title,'下一首');
});
test('decorated long row titles and stale tooltips do not hide the real official song identity',()=>{
    const f=fixture();f.session.metadata=null;const title='风吹麦浪（现场特别版）';playerDOM(f,title,'longSong123');
    f.nodes.set('.songlist__item--playing .songlist__songname_txt',node(`${title} MV VIP 原声带精选`));
    const link=f.nodes.get('.songlist__item--playing .songlist__songname_txt a');
    link.childNodes=[{nodeType:3,textContent:title},{nodeType:1,textContent:'MV'}];
    link.textContent=`${title}MV`;link.getAttribute=key=>({href:'/n/ryqq/songDetail/longSong123',title:'Back at One'}[key]||null);
    f.lists.set('#qrc_ctn p',[node('真实当前歌词',{},['on'])]);
    const projected=f.bridge.projection();assert.equal(projected.media.title,title);assert.equal(projected.media.track_key,'qq:longSong123');assert.equal(projected.lyrics.title,title);
    f.nodes.delete('.songlist__item--playing .songlist__songname_txt a');f.nodes.delete('.song_info__name a');
    const transport=node('安和桥',{href:'/n/ryqq_v2/songDetail/anotherSong',title:'Back at One'});
    transport.childNodes=[{nodeType:3,textContent:'安和桥'}];f.nodes.set('.player_music__info a',transport);
    assert.equal(f.bridge.projection().media.title,'安和桥','visible transport text wins over stale tooltip');
});
test('row and sidebar hydration never combine different song metadata, artwork, lyrics or clock',()=>{
    const f=fixture();f.session.metadata={title:'旧MediaSession标题',artist:'旧歌手',album:'旧专辑'};playerDOM(f,'旧歌','oldSong');
    f.lists.set('#qrc_ctn p',[node('旧歌歌词',{},['on'])]);f.bridge.projection();
    f.nodes.set('.songlist__item--playing .songlist__songname_txt a',node('新歌',{href:'/n/ryqq_v2/songDetail/newSong'}));
    f.lists.set('.songlist__item--playing .songlist__artist a.playlist__author',[node('新歌手')]);
    f.nodes.set('.songlist__item--playing .songlist__time',node('03:59'));
    let p=f.bridge.projection();assert.equal(p.media.title,'新歌');assert.equal(p.media.artist,'新歌手');assert.equal(p.media.album,'');assert.equal(p.media.artwork,'');assert.equal(p.media.position,0);assert.equal(p.media.duration,239);assert.equal(p.lyrics,null);
    playerDOM(f,'新歌','newSong');p=f.bridge.projection();assert.equal(p.media.title,'新歌');assert.equal(p.lyrics,null,'old lyric fingerprint still belongs to the previous identity');
    f.lists.set('#qrc_ctn p',[node('新歌歌词',{},['on'])]);p=f.bridge.projection();assert.equal(p.lyrics.title,'新歌');assert.equal(p.lyrics.current,0);
    f.lists.set('.song_info__singer a.playlist__author',[node('新歌手 完整信息')]);f.nodes.set('.song_info__album a',node('异步专辑'));
    const hydrated=f.bridge.projection();assert.equal(hydrated.media.track_key,p.media.track_key);assert.equal(hydrated.lyrics.title,'新歌','same song metadata hydration must not permanently suppress unchanged lyrics');
});
test('either sidebar or active row can hydrate first without old nodes switching the title back',()=>{
    const f=fixture();playerDOM(f,'旧歌','oldIdentity');f.bridge.projection();
    f.nodes.set('.song_info__name a',node('新歌',{href:'/n/ryqq_v2/songDetail/newIdentity'}));
    f.nodes.set('.player_music__info a',node('新歌',{href:'/n/ryqq_v2/songDetail/newIdentity'}));
    for(let i=0;i<4;i++)assert.equal(f.bridge.projection().media.title,'新歌','stale active marker cannot override newer visible sidebar and transport');
    playerDOM(f,'新歌','newIdentity');f.bridge.projection();
    f.nodes.set('.songlist__item--playing .songlist__songname_txt a',node('第三首',{href:'/n/ryqq/songDetail/thirdIdentity'}));
    for(let i=0;i<4;i++)assert.equal(f.bridge.projection().media.title,'第三首','stale sidebar pair cannot reverse a row-first transition');
    playerDOM(f,'第三首','thirdIdentity');f.bridge.projection();
    playerDOM(f,'旧歌','oldIdentity');assert.equal(f.bridge.projection().media.title,'旧歌','normal Previous remains available after hydration converges');
});
test('old detached paused media cannot override a later DOM-only track and metadata updates remain bounded',async()=>{
    const f=fixture('https://y.qq.com',false,{now:1000});playerDOM(f,'第一首','firstTrack');const audio=new f.Media();await audio.play();f.bridge.projection();
    audio.pause();playerDOM(f,'第二首','secondTrack');f.lists.set('#qrc_ctn p',[node('第二首当前歌词',{},['on'])]);
    const p=f.bridge.projection();assert.equal(p.media.playing,true);assert.equal(p.media.position,83);assert.equal(p.media.duration,245);assert.equal(p.media.volume,null);
    for(let i=0;i<20;i++)f.bridge.projection();assert.equal(f.contentObserverCount(),1,'rehydrated roots share one content observer');
    f.window.dispatchEvent(new Event('pagehide'));assert.equal(f.contentObserverCount(),0);
    const departure=f.messages.at(-1);assert.equal(departure.document,p.document);assert.equal(departure.media,null);assert.ok(departure.sequence>p.sequence);
});
test('profile playlists remain available without playback and only trusted observed playlist identities enter the cache', () => {
    const f=fixture();f.location.pathname='/n/ryqq_v2/profile/like/playlist';
    const favorites=node('我喜欢',{href:'/n/ryqq_v2/profile/like/song'}),created=node('我创建的歌单',{href:'/n/ryqq_v2/profile/create'});
    f.nodes.set('#like_tab',favorites);f.nodes.set('#create_tab',created);
    f.lists.set('a.mod_tab__item',[favorites,created,node('歌单 11',{href:'/n/ryqq_v2/profile/like/playlist'})]);
    f.lists.set('a[href*="/playlist/"]',[node('收藏歌单',{href:'/n/ryqq_v2/playlist/12345'}),node('外部链接',{href:'https://evil.test/n/ryqq_v2/playlist/99'}),node('非法标识',{href:'/n/ryqq_v2/playlist/not-numeric'})]);
    const p=f.bridge.projection();assert.equal(p.media,null);assert.equal(p.library.logged_in,true);assert.equal(p.library.section,'collected');
    assert.deepEqual(Array.from(p.library.playlists,p=>p.id),['favorites','12345']);assert.equal(f.bridge.libraryRoutes().length,3);
    f.location.pathname='/n/ryqq_v2/playlist/12345';let clicked=0;
    const play=node('播放全部');play.click=()=>clicked++;f.lists.set('a.mod_btn_green',[play]);
    assert.equal(f.bridge.activatePlaylist('https://y.qq.com/n/ryqq_v2/playlist/12345'),true);assert.equal(clicked,1);
    assert.equal(f.bridge.activatePlaylist('https://y.qq.com/n/ryqq_v2/playlist/99999'),false);assert.equal(clicked,1);
    f.location.origin='https://graph.qq.com';assert.equal(f.bridge.activatePlaylist('https://y.qq.com/n/ryqq_v2/playlist/12345'),false);
});
function absolute(f,playing,extra={}) {
    const value=f.bridge.projection();
    return {operation:6,value:playing?1:0,document:value.document,track_key:value.media.track_key,deadline:Date.now()+1200,...extra};
}
function playbackButton(f,playing,click) {
    const button=node(playing?'暂停':'播放',{},playing?['btn_big_play--pause']:[]);button.click=click;f.nodes.set('a.btn_big_play',button);
}
test('absolute DOM playback confirms a no-op immediately and rapid pause/play/pause does not double-toggle stale DOM', async () => {
    const f=fixture('https://y.qq.com',true);f.session.metadata=null;playerDOM(f);let clicks=0;
    const click=()=>{clicks++;const desired=!f.bridge.snapshot().playing;setTimeout(()=>{playbackButton(f,desired,click);f.notify();},12);};
    playbackButton(f,true,click);
    const start=performance.now();assert.equal(await f.bridge.command(absolute(f,true)),true);
    assert.equal(clicks,0);assert.equal(f.messages.at(-1).media.playing,true);
    const pause=f.bridge.command(absolute(f,false));
    const play=f.bridge.command(absolute(f,true));
    const finalPause=f.bridge.command(absolute(f,false));
    const results=await Promise.all([pause,play,finalPause]);
    assert.equal(results[2],true);assert.equal(results[1],false);assert.equal(f.bridge.snapshot().playing,false);
    assert.equal(clicks,1);assert.equal(f.messages.at(-1).media.playing,false);assert.equal(f.observerCount(),0);
    assert.ok(performance.now()-start<250,'confirmation bypasses the 100ms event delay and 1s heartbeat');
});
test('absolute controls reject stale document, track and deadline before clicking, and original audio toggle still confirms', async () => {
    const f=fixture('https://y.qq.com',true);playerDOM(f);let clicks=0;playbackButton(f,true,()=>clicks++);
    for(const extra of [{document:'old-document'},{track_key:'old-track'},{deadline:Date.now()-1}]){
        assert.equal(await f.bridge.command(absolute(f,false,extra)),false);
    }
    assert.equal(clicks,0);
    const audioFixture=fixture('https://y.qq.com',true);audioFixture.buttons.delete('a.btn_big_play');
    const audio=new audioFixture.Media();await audio.play();
    assert.equal(await audioFixture.bridge.command(request(0)),true);assert.equal(audio.paused,true);
    assert.equal(await audioFixture.bridge.command(request(0)),true);assert.equal(audio.paused,false);
});
test('unconfirmed clicks time out and a quick retry cannot send a second toggle against the old class', async () => {
    const f=fixture('https://y.qq.com',true);f.session.metadata=null;playerDOM(f);let clicks=0;
    playbackButton(f,true,()=>clicks++);const started=performance.now();
    assert.equal(await f.bridge.command(absolute(f,false)),false);
    assert.equal(await f.bridge.command(absolute(f,false)),false);
    assert.equal(clicks,1);assert.equal(f.bridge.snapshot().playing,true);assert.equal(f.observerCount(),0);
    assert.ok(performance.now()-started<1100);
});
test('missing profile DOM after five seconds, offline pages and hidden login frames never mean expired auth',()=>{
    const clock={now:0},f=fixture('https://y.qq.com',false,clock);f.location.pathname='/n/ryqq_v2/profile/like/song';
    f.document.readyState='loading';clock.now=6000;
    assert.equal(f.bridge.projection().library.auth,'restoring');assert.equal(f.bridge.projection().library.loading,true);
    f.document.readyState='complete';clock.now=30000;
    assert.equal(f.bridge.projection().library.auth,'unknown');assert.equal(f.bridge.projection().library.logged_in,false);
    const login=node('',{src:'https://graph.qq.com/oauth2.0/show'});login.tagName='IFRAME';login.hidden=true;
    f.lists.set('a.mod_btn_green,button,iframe',[login]);clock.now=40000;
    assert.equal(f.bridge.projection().library.auth,'unknown');
    login.hidden=false;f.navigator.onLine=false;clock.now=50000;
    assert.equal(f.bridge.projection().library.auth,'restoring');assert.notEqual(f.bridge.projection().library.auth,'expired');
});
test('only a stable visible official login marker expires auth; authenticated profile markers restore it',()=>{
    const clock={now:1000},f=fixture('https://y.qq.com',false,clock);f.location.pathname='/n/ryqq_v2/profile/like/song';f.document.readyState='complete';
    const login=node('',{src:'https://graph.qq.com/oauth2.0/show'});login.tagName='IFRAME';f.lists.set('a.mod_btn_green,button,iframe',[login]);
    assert.equal(f.bridge.projection().library.auth,'restoring');clock.now=2600;
    assert.equal(f.bridge.projection().library.auth,'expired');assert.equal(f.bridge.projection().library.loading,false);
    f.nodes.set('#create_tab',node('我创建的歌单'));f.nodes.set('#like_tab',node('我喜欢'));
    assert.equal(f.bridge.projection().library.auth,'connected');assert.equal(f.bridge.projection().library.logged_in,true);
    f.nodes.delete('#create_tab');f.nodes.delete('#like_tab');login.getAttribute=()=> 'https://evil.test/oauth';clock.now=10000;
    assert.equal(f.bridge.projection().library.auth,'unknown');
});
const resumeIntent=(extra={})=>({request:1,track_url:'https://y.qq.com/n/ryqq_v2/songDetail/abc123',playlist_url:'',position:0,preserve_position:false,accept_any:false,deadline:Date.now()+2000,...extra});
test('loading cached playback never starts audio; exact retained track resumes and position is truthful',async()=>{
    const f=fixture();playerDOM(f);let clicks=0;f.nodes.get('a.btn_big_play').click=()=>clicks++;
    f.bridge.projection();f.bridge.projection();assert.equal(clicks,0,'memory projection does not autoplay');
    const playing=await f.bridge.resumeStep(resumeIntent({position:83}));
    assert.equal(playing.status,'playing');assert.equal(playing.seek,-1,'DOM-only QQ cannot claim restored seek');assert.equal(clicks,0);
    const audio=new f.Media();await audio.play();audio.currentTime=12;
    const restored=await f.bridge.resumeStep(resumeIntent({request:2,position:42}));
    assert.equal(restored.status,'playing');assert.equal(restored.seek,1);assert.equal(audio.currentTime,42);
    audio.currentTime=57;
    const reused=await f.bridge.resumeStep(resumeIntent({request:3,position:42,preserve_position:true}));
    assert.equal(reused.status,'playing');assert.equal(audio.currentTime,57,'live player retains current progress');
    assert.equal((await f.bridge.resumeStep(resumeIntent({request:1}))).status,'stale');
});
test('cold exact resume clicks only the public row whose song URL matches, even when titles match',async()=>{
    const f=fixture();playerDOM(f,'同名歌曲','other456');let wrong=0,right=0;
    const wrongLink=node('同名歌曲',{href:'/n/ryqq_v2/songDetail/other456'});wrongLink.closest=()=>({querySelector:()=>({isConnected:true,getAttribute:()=>null,click:()=>wrong++})});
    const rightLink=node('同名歌曲',{href:'/n/ryqq/songDetail/abc123'});rightLink.closest=()=>({querySelector:()=>({isConnected:true,getAttribute:()=>null,click:()=>{right++;playerDOM(f,'同名歌曲','abc123');}})});
    f.lists.set('.songlist__songname_txt a',[wrongLink,rightLink]);
    assert.equal((await f.bridge.resumeStep(resumeIntent())).status,'requested');assert.equal(wrong,0);assert.equal(right,1);
    assert.equal((await f.bridge.resumeStep(resumeIntent())).status,'playing');assert.equal(right,1);
    assert.equal((await f.bridge.resumeStep(resumeIntent({request:2,track_url:'https://evil.test/n/ryqq/songDetail/abc123'}))).status,'invalid');
});
test('cached playlist action uses its own official page; exact detail fallback never picks a different recording',async()=>{
    const f=fixture();f.location.pathname='/n/ryqq_v2/playlist/12345';f.location.href='https://y.qq.com'+f.location.pathname;let clicks=0;
    const play=node('播放全部');play.click=()=>clicks++;f.lists.set('a.mod_btn_green',[play]);
    const cached=resumeIntent({track_url:'',playlist_url:'https://y.qq.com/n/ryqq_v2/playlist/12345'});
    assert.equal((await f.bridge.resumeStep(cached)).status,'requested');assert.equal(clicks,1);
    assert.equal((await f.bridge.resumeStep(cached)).status,'requested');assert.equal(clicks,1,'do not repeat a submitted click');
    const detail=fixture();detail.location.pathname='/n/ryqq_v2/songDetail/abc123';detail.location.href='https://y.qq.com'+detail.location.pathname;
    const detailPlay=node('播放');let detailClicks=0;detailPlay.click=()=>detailClicks++;detail.lists.set('a.mod_btn_green,button.mod_btn_green',[detailPlay]);
    assert.equal((await detail.bridge.resumeStep(resumeIntent())).status,'requested');assert.equal(detailClicks,1);
    assert.equal((await detail.bridge.resumeStep(resumeIntent({request:2,track_url:'https://y.qq.com/n/ryqq/songDetail/other456'}))).status,'waiting');assert.equal(detailClicks,1);
    assert.equal((await detail.bridge.resumeStep(resumeIntent({request:3,deadline:Date.now()-1}))).status,'stale');assert.equal(detailClicks,1);
});
test('old currently playing queue cannot confirm a different cached playlist before its playback request',async()=>{
    const f=fixture();playerDOM(f);const target=resumeIntent({track_url:'',playlist_url:'https://y.qq.com/n/ryqq_v2/playlist/12345'});
    assert.notEqual((await f.bridge.resumeStep(target)).status,'playing');
    assert.equal((await f.bridge.resumeStep({...target,accept_any:true})).status,'playing');
});
