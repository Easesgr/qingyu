// Real WebKit regression. Uses local silent PCM; never reads account storage.
// xcrun clang -fobjc-arc -fblocks -mmacosx-version-min=12.0 \
//   -framework AppKit -framework WebKit tests/qq_music_single_playback.m \
//   -o /tmp/qingyu-qq-single-playback && /tmp/qingyu-qq-single-playback
#import "../src/macos/qq_music.m"

static NSUInteger checks,failures;
static NSMutableDictionary<NSNumber *,NSNumber *> *receipts;
// Production navigation deliberately rejects srcdoc. This fixture-only
// delegate permits the local synthetic child while forwarding every real
// lifecycle callback to the production coordinator.
@interface QYQQFixtureFrameDelegate : NSObject <WKNavigationDelegate>
@property(weak) QYQQWindow *owner;
@end
@implementation QYQQFixtureFrameDelegate
- (BOOL)respondsToSelector:(SEL)selector{return [super respondsToSelector:selector] || [self.owner respondsToSelector:selector];}
- (id)forwardingTargetForSelector:(SEL)selector{return self.owner;}
- (void)webView:(WKWebView *)view decidePolicyForNavigationAction:(WKNavigationAction *)action decisionHandler:(void (^)(WKNavigationActionPolicy))decision {
    if([action.request.URL.absoluteString isEqual:@"about:srcdoc"]){decision(WKNavigationActionPolicyAllow);return;}
    [self.owner webView:view decidePolicyForNavigationAction:action decisionHandler:decision];
}
@end
static void check(BOOL condition,const char *message){checks++;if(!condition){failures++;fprintf(stderr,"FAIL: %s\n",message);}}
void qy_qq_media_reset(uint64_t view,uint64_t epoch){}
void qy_qq_media_remove(uint64_t view){}
void qy_qq_media_update(uint64_t view,uint64_t epoch,const uint8_t *bytes,size_t length){}
void qy_qq_projection_update(uint64_t view,uint64_t epoch,const uint8_t *bytes,size_t length){}
void qy_qq_library_status(const char *message){}
void qy_qq_restoration_failed(const char *message){}
void qy_qq_cache_clear(void){}
void qy_qq_playing_result(uint64_t request,bool success){receipts[@(request)]=@(success);}
void qy_qq_start_result(uint64_t request,bool success,int32_t seek,const char *message){receipts[@(request)]=@(success);}

static BOOL until(BOOL (^condition)(void),double seconds){
    NSDate *limit=[NSDate dateWithTimeIntervalSinceNow:seconds];
    while(!condition() && limit.timeIntervalSinceNow>0)[NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
    return condition();
}
static id js(WKWebView *view,NSString *script){
    __block BOOL done=NO;__block id result=nil;
    [view evaluateJavaScript:script completionHandler:^(id value,NSError *error){result=error?nil:value;done=YES;}];
    check(until(^BOOL{return done;},3),"WebKit JavaScript callback completes");return result;
}
static WKMediaPlaybackState state(WKWebView *view){
    __block BOOL done=NO;__block WKMediaPlaybackState result=WKMediaPlaybackStateNone;
    [view requestMediaPlaybackStateWithCompletionHandler:^(WKMediaPlaybackState value){result=value;done=YES;}];
    check(until(^BOOL{return done;},3),"public WebKit media-state callback completes");return result;
}
static NSString *wav(void){
    uint32_t rate=8000,frames=rate*60,size=36+frames,fmt=16;uint16_t one=1,eight=8;
    NSMutableData *data=[NSMutableData data];
    [data appendBytes:"RIFF" length:4];[data appendBytes:&size length:4];[data appendBytes:"WAVEfmt " length:8];
    [data appendBytes:&fmt length:4];[data appendBytes:&one length:2];[data appendBytes:&one length:2];
    [data appendBytes:&rate length:4];[data appendBytes:&rate length:4];[data appendBytes:&one length:2];[data appendBytes:&eight length:2];
    [data appendBytes:"data" length:4];[data appendBytes:&frames length:4];
    NSMutableData *silence=[NSMutableData dataWithLength:frames];memset(silence.mutableBytes,128,frames);[data appendData:silence];
    return [@"data:audio/wav;base64," stringByAppendingString:[data base64EncodedStringWithOptions:0]];
}
static NSString *html(BOOL autoplay){
    return [NSString stringWithFormat:@"<html><body><div class='song_info__name'><a href='/n/ryqq/songDetail/fixture'>Fixture</a></div><script>window.sources=[new Audio('%@')];window.tryPlay=()=>sources[0].play().catch(()=>{});%@</script></body></html>",wav(),autoplay?@"tryPlay();":@""];
}
static QYQQCoordinator *host(void){
    QYQQCoordinator *owner=[QYQQCoordinator new];owner.windows=[NSMutableArray new];owner.profile=@"single-playback-regression";
    owner.store=WKWebsiteDataStore.nonPersistentDataStore;
    NSString *path=[NSString stringWithUTF8String:__FILE__];
    owner.script=[NSString stringWithContentsOfFile:[[path stringByDeletingLastPathComponent]stringByAppendingPathComponent:@"../src/macos/qq_music.js"] encoding:NSUTF8StringEncoding error:nil];
    check(owner.script.length>0,"loads the production bridge");return owner;
}
static void ready(QYQQWindow *window){
    check(until(^BOOL{return window.ready && window.documentToken.length;},5),"fixture document finishes and establishes its native epoch");
}
static void playing(QYQQCoordinator *owner,QYQQWindow *window,BOOL nativeState){
    check(until(^BOOL{return owner.audioOwner==window && !owner.audioBusy && !window.pendingAudioRequest;},5),"candidate obtains the completed native audio lease");
    check(until(^BOOL{return [js(window.webView,@"window.__qingyuQQMusic?.snapshot()?.playing===true") boolValue];},3),"the approved source actually plays");
    if(nativeState)check(until(^BOOL{return state(window.webView)==WKMediaPlaybackStatePlaying;},3),"public WebKit state confirms real media playback");
    double position=[js(window.webView,@"window.__qingyuQQMusic.snapshot()?.position||0") doubleValue];
    check(until(^BOOL{return [js(window.webView,@"window.__qingyuQQMusic.snapshot()?.position||0") doubleValue]>position+0.15;},2),"approved media time advances while the background window stays hidden");
}
static void transport(QYQQCoordinator *owner,QYQQWindow *window,BOOL play,uint64_t identifier){
    [window poll];until(^BOOL{return !window.pollBusy;},2);
    QYQQPlaybackRequest *request=[QYQQPlaybackRequest new];request.requestID=identifier;request.viewID=window.viewID;
    request.epoch=window.epoch;request.document=window.documentToken;request.track=window.trackKey;request.playing=play;
    request.expires=NSProcessInfo.processInfo.systemUptime+1.2;[owner queuePlayback:request];
    check(until(^BOOL{return receipts[@(identifier)]!=nil;},3),"native transport receives a bounded receipt");
    check([receipts[@(identifier)] boolValue],play?"explicit resume succeeds":"explicit pause succeeds");
}
static void controlReproducesDoublePlayback(void){
    NSMutableArray<NSWindow *> *windows=[NSMutableArray new];NSMutableArray<WKWebView *> *views=[NSMutableArray new];
    for(NSUInteger index=0;index<2;index++){
        WKWebViewConfiguration *configuration=[WKWebViewConfiguration new];configuration.websiteDataStore=WKWebsiteDataStore.nonPersistentDataStore;
        configuration.mediaTypesRequiringUserActionForPlayback=WKAudiovisualMediaTypeNone;
        NSWindow *window=[[NSWindow alloc]initWithContentRect:NSMakeRect(0,0,400,300) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
        window.releasedWhenClosed=NO;WKWebView *view=[[WKWebView alloc]initWithFrame:NSMakeRect(0,0,400,300) configuration:configuration];
        window.contentView=view;[windows addObject:window];[views addObject:view];[view loadHTMLString:html(YES) baseURL:[NSURL URLWithString:QYQQPlayerURL]];
    }
    for(WKWebView *view in views)check(until(^BOOL{return [js(view,@"!!window.sources && !sources[0].paused && sources[0].readyState>0") boolValue];},5),"uncoordinated control fixture starts its audio");
    check(state(views[0])==WKMediaPlaybackStatePlaying && state(views[1])==WKMediaPlaybackStatePlaying,"control reproduces two simultaneous real WebKit audio sources");
    fprintf(stdout,"CONTROL: two uncoordinated WebViews both report Playing\n");
    for(WKWebView *view in views){js(view,@"sources[0].pause()");[view setAllMediaPlaybackSuspended:YES completionHandler:nil];[view stopLoading];}
    for(NSWindow *window in windows)[window close];
}
int main(void){@autoreleasepool{
    [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];[NSApp finishLaunching];
    receipts=[NSMutableDictionary new];controlReproducesDoublePlayback();
    QYQQCoordinator *owner=host();QYQQWindow *first=[owner create:1 configuration:[WKWebViewConfiguration new]];
    first.audioSelection=[owner beginAudioSelection];[first.webView loadHTMLString:html(YES) baseURL:[NSURL URLWithString:QYQQPlayerURL]];ready(first);playing(owner,first,YES);
    WKWebView *firstView=first.webView;
    // A library enumeration cannot steal the retained player's lease.
    QYQQWindow *library=[owner hiddenLibrary];[library.webView loadHTMLString:html(NO) baseURL:[NSURL URLWithString:QYQQLibraryURL]];ready(library);
    js(library.webView,@"tryPlay();true");
    until(^BOOL{return library.pendingAudioRequest==0;},2);
    check(state(library.webView)!=WKMediaPlaybackStatePlaying && state(firstView)==WKMediaPlaybackStatePlaying,"ordinary library work stays silent while the owner keeps playing");
    // Real window.open('about:blank') must return a usable official popup.
    library.audioSelection=[owner beginAudioSelection];
    check([js(library.webView,@"window.fixturePopup=window.open('about:blank','qingyu-fixture');!!fixturePopup") boolValue],"blank popup is returned instead of dropping the site's queue delivery");
    QYQQWindow *second=owner.windows.lastObject;
    check(second!=library && second!=first && second.audioSelection==owner.audioSelection,"popup inherits only its current playback selection");
    [second.webView loadHTMLString:html(YES) baseURL:[NSURL URLWithString:QYQQPlayerURL]];
    js(firstView,@"tryPlay();true");ready(second);playing(owner,second,YES);
    check(state(firstView)!=WKMediaPlaybackStatePlaying,"handoff physically pauses the preceding WebView");
    js(firstView,@"setTimeout(tryPlay,10);true");
    until(^BOOL{return first.disposed;},2);
    check(state(firstView)!=WKMediaPlaybackStatePlaying && state(second.webView)==WKMediaPlaybackStatePlaying,"late old-owner play cannot mix with its successor");
    [owner releaseLibraryWorker];check(library.disposed,"the completed playlist opener is retired");
    // Selecting again invalidates a worker even if its play request arrives late.
    QYQQWindow *stale=[owner hiddenLibrary];stale.audioSelection=[owner beginAudioSelection];
    [stale.webView loadHTMLString:html(NO) baseURL:[NSURL URLWithString:@"https://y.qq.com/n/ryqq/playlist/123"]];ready(stale);
    QYQQWindow *latest=[owner create:1 configuration:[WKWebViewConfiguration new]];latest.audioSelection=[owner beginAudioSelection];
    js(stale.webView,@"tryPlay();true");
    [latest.webView loadHTMLString:html(YES) baseURL:[NSURL URLWithString:QYQQPlayerURL]];ready(latest);playing(owner,latest,YES);
    check(state(stale.webView)!=WKMediaPlaybackStatePlaying,"a superseded worker cannot reclaim playback after the newer selection");
    check(state(second.webView?:firstView)!=WKMediaPlaybackStatePlaying,"the newest selection has one active source");
    // Same document and same-origin frame sources also share the single lease.
    js(latest.webView,[NSString stringWithFormat:@"sources.push(new Audio('%@'));sources[1].play().catch(()=>{});true",wav()]);
    check([js(latest.webView,@"sources.filter(a=>!a.paused).length") integerValue]==1,"two detached media elements cannot play together");
    QYQQFixtureFrameDelegate *frameDelegate=[QYQQFixtureFrameDelegate new];frameDelegate.owner=latest;latest.webView.navigationDelegate=frameDelegate;
    js(latest.webView,[NSString stringWithFormat:@"window.fixtureFrame=document.createElement('iframe');fixtureFrame.srcdoc='<script>window.a=new Audio(\"%@\");a.play().catch(()=>{});</script>';document.body.append(fixtureFrame);true",wav()]);
    check(until(^BOOL{return [js(latest.webView,@"!!fixtureFrame.contentWindow.a && !fixtureFrame.contentWindow.a.paused") boolValue];},4),"same-origin child audio can use the owner's lease");
    latest.webView.navigationDelegate=latest;
    check([js(latest.webView,@"sources.filter(a=>!a.paused).length") integerValue]==0,"child playback pauses the top document's previous source");
    transport(owner,latest,NO,100);
    check(until(^BOOL{return !owner.audioBusy && !owner.audioOwner;},3),"pause completes the native suspension gate");
    js(latest.webView,@"tryPlay();fixtureFrame.contentWindow.a.play().catch(()=>{});true");
    until(^BOOL{return latest.pendingAudioRequest==0;},2);
    check(state(latest.webView)!=WKMediaPlaybackStatePlaying,"late page or child callbacks cannot revive a user pause");
    // WebKit 15 reports Paused after resuming a top source with a paused child;
    // confirm the actual selected media's advancing clock in this case.
    transport(owner,latest,YES,101);playing(owner,latest,NO);
    check([js(latest.webView,@"sources.filter(a=>!a.paused).length===1 && fixtureFrame.contentWindow.a.paused") boolValue],"explicit resume leaves every other top/child source paused");
    WKWebView *latestView=latest.webView,*staleView=stale.webView;
    QYQQWindow *contextWindow=[owner create:1 configuration:[WKWebViewConfiguration new]];contextWindow.audioSelection=[owner beginAudioSelection];
    check(contextWindow!=nil,"creates a Web Audio fallback candidate within the window bound");
    NSString *contextHTML=@"<html><body><div class='song_info__name'><a href='/n/ryqq/songDetail/context'>Context</a></div><a class='btn_big_play btn_big_play--pause'>暂停</a><script>window.fixtureContext=new AudioContext();const gain=fixtureContext.createGain();gain.gain.value=0;gain.connect(fixtureContext.destination);const oscillator=fixtureContext.createOscillator();oscillator.connect(gain);oscillator.start();fixtureContext.resume().catch(()=>{});</script></body></html>";
    [contextWindow.webView loadHTMLString:contextHTML baseURL:[NSURL URLWithString:QYQQPlayerURL]];ready(contextWindow);
    check(until(^BOOL{[contextWindow poll];return owner.audioOwner==contextWindow && !owner.audioBusy;},5),"public suspended Web Audio attempt can acquire the selected player's lease");
    check([js(contextWindow.webView,@"fixtureContext.state==='running'") boolValue],"fallback leaves the actual AudioContext running");
    double contextTime=[js(contextWindow.webView,@"fixtureContext.currentTime") doubleValue];
    check(until(^BOOL{return [js(contextWindow.webView,@"fixtureContext.currentTime") doubleValue]>contextTime+0.15;},2),"fallback audio clock advances without muting the whole WebView");
    check([js(latestView,@"sources.every(a=>a.paused) && fixtureFrame.contentWindow.a.paused") boolValue],"Web Audio takeover pauses the prior page's real sources");
    WKWebView *contextView=contextWindow.webView;
    [owner shutdown];
    check(until(^BOOL{return state(latestView)!=WKMediaPlaybackStatePlaying && state(staleView)!=WKMediaPlaybackStatePlaying && state(contextView)!=WKMediaPlaybackStatePlaying;},3),"stop closes every owned playback page");
    js(latestView,@"tryPlay();true");check(state(latestView)!=WKMediaPlaybackStatePlaying,"stopped documents cannot restart themselves");
    check(owner.windows.count==0,"shutdown releases all music windows without changing the website data store");
    fprintf(stdout,"single playback: %lu checks; %lu failures\n",(unsigned long)checks,(unsigned long)failures);
    return failures?1:0;
}}
