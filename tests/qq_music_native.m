// Standalone native playback regression; uses only local silent PCM audio.
// xcrun clang -fobjc-arc -fblocks -mmacosx-version-min=12.0 \
//   -framework AppKit -framework WebKit tests/qq_music_native.m -o /tmp/qingyu-qq-native-test
// /tmp/qingyu-qq-native-test
#import "../src/macos/qq_music.m"

static NSUInteger failures, resets, projections;
static NSDictionary *lastProjection;
static void check(BOOL condition, const char *message) {
    if (!condition) { failures++; fprintf(stderr,"FAIL: %s\n",message); }
}
void qy_qq_media_reset(uint64_t view,uint64_t epoch) { resets++; lastProjection=nil; }
void qy_qq_media_remove(uint64_t view) {}
void qy_qq_media_update(uint64_t view,uint64_t epoch,const uint8_t *bytes,size_t length) {}
void qy_qq_projection_update(uint64_t view,uint64_t epoch,const uint8_t *bytes,size_t length) {
    projections++; lastProjection=[NSJSONSerialization JSONObjectWithData:[NSData dataWithBytes:bytes length:length] options:0 error:nil];
}
void qy_qq_library_status(const char *message) {}
void qy_qq_restoration_failed(const char *message) {}
void qy_qq_cache_clear(void) {}
void qy_qq_playing_result(uint64_t request,bool success) {}
void qy_qq_start_result(uint64_t request,bool success,int32_t seek,const char *message) {}

@interface QYQQHeldWebView : WKWebView
@property(strong) NSMutableArray *callbacks;
@end
@implementation QYQQHeldWebView
- (NSURL *)URL { return [NSURL URLWithString:QYQQPlayerURL]; }
- (void)evaluateJavaScript:(NSString *)script completionHandler:(void (^)(id,NSError *))completion {
    if(!self.callbacks)self.callbacks=[NSMutableArray new];
    [self.callbacks addObject:[completion copy]];
}
@end

static QYQQCoordinator *host(void) {
    QYQQCoordinator *owner=[QYQQCoordinator new];owner.windows=[NSMutableArray new];
    owner.profile=@"native-regression";owner.store=WKWebsiteDataStore.nonPersistentDataStore;
    NSString *path=[NSString stringWithUTF8String:__FILE__];
    owner.script=[NSString stringWithContentsOfFile:[[path stringByDeletingLastPathComponent] stringByAppendingPathComponent:@"../src/macos/qq_music.js"] encoding:NSUTF8StringEncoding error:nil];
    check(owner.script.length>0,"read production document bridge");return owner;
}
static NSDictionary *projection(NSString *document,NSUInteger sequence,BOOL playing) {
    return @{@"origin":@"https://y.qq.com",@"document":document,@"sequence":@(sequence),@"library":NSNull.null,@"lyrics":NSNull.null,
        @"media":@{@"origin":@"https://y.qq.com",@"title":@"Native fixture",@"artist":@"",@"album":@"",@"artwork":@"",
        @"track_key":@"fixture",@"track_url":@"",@"playing":@(playing),@"position":@1,@"duration":@30,@"volume":@0.4,@"muted":@NO}};
}
static void testSlowPoll(void) {
    QYQQCoordinator *owner=host();QYQQWindow *window=[owner create:1 configuration:[WKWebViewConfiguration new]];
    QYQQHeldWebView *view=[[QYQQHeldWebView alloc]initWithFrame:NSZeroRect];window.webView=view;window.ready=YES;
    owner.audioOwner=window;window.audioSuspended=NO;
    [window acceptSnapshot:projection(@"document-a",1,YES)];
    [window poll];check(view.callbacks.count==1 && window.pollBusy,"one in-flight poll");
    NSUInteger before=resets;window.pollStarted=NSProcessInfo.processInfo.systemUptime-6;
    [window poll];[window poll];
    check(resets==before,"slow read must not reset the still-live playback document");
    check(view.callbacks.count==1,"slow reads must not enqueue more WebKit work");
    [window acceptSnapshot:projection(@"document-a",2,YES)];
    check([lastProjection[@"sequence"] unsignedIntegerValue]==2,"heartbeat can update playback during a slow poll");
    void (^oldCompletion)(id,NSError *)=view.callbacks.firstObject;
    [window invalidateDocument];window.ready=YES;[window poll];
    oldCompletion(projection(@"document-a",3,NO),nil);
    check(window.pollBusy && !window.documentToken,"old navigation callback cannot settle a newer poll");
    void (^newCompletion)(id,NSError *)=view.callbacks.lastObject;
    newCompletion(projection(@"document-b",1,YES),nil);
    check(!window.pollBusy && [window.documentToken isEqual:@"document-b"],"new document establishes its own identity");
    [owner shutdown];
}
static void testPlayerOwnership(void) {
    QYQQCoordinator *owner=host();QYQQWindow *player=[owner create:1 configuration:[WKWebViewConfiguration new]];
    [player webViewDidClose:player.webView];
    check(!player.disposed && [owner.windows containsObject:player],"script close retains a player before metadata or during a track gap");
    QYQQWindow *worker=[owner hiddenLibrary];worker.role=1;
    [owner releaseLibraryWorker];
    check(worker.disposed && !owner.libraryWorker,"a page without an audio lease cannot survive worker release as an extra player");
    QYQQWindow *mediaWorker=[owner hiddenLibrary];mediaWorker.hasMedia=YES;owner.audioOwner=mediaWorker;
    QYQQWindow *replacement=[owner hiddenLibrary];
    check(!mediaWorker.disposed && replacement && replacement!=mediaWorker,"library refresh cannot navigate an audio-owning worker");
    [owner releaseLibraryWorker];
    check(replacement.disposed,"unused temporary pages are still released");
    [owner shutdown];
}
static NSString *silentWAV(void) {
    uint32_t rate=8000,frames=rate*30,size=36+frames;uint16_t one=1,eight=8;
    NSMutableData *data=[NSMutableData data];
    [data appendBytes:"RIFF" length:4];[data appendBytes:&size length:4];[data appendBytes:"WAVEfmt " length:8];
    uint32_t fmt=16;[data appendBytes:&fmt length:4];[data appendBytes:&one length:2];[data appendBytes:&one length:2];
    [data appendBytes:&rate length:4];[data appendBytes:&rate length:4];[data appendBytes:&one length:2];[data appendBytes:&eight length:2];
    [data appendBytes:"data" length:4];[data appendBytes:&frames length:4];
    NSMutableData *silence=[NSMutableData dataWithLength:frames];memset(silence.mutableBytes,128,frames);[data appendData:silence];
    return [@"data:audio/wav;base64," stringByAppendingString:[data base64EncodedStringWithOptions:0]];
}
static QYQQCoordinator *realHost;
static QYQQWindow *realWindow;
static double previousPosition;
static NSUInteger observed,startedSamples;
static BOOL observedGap,observedSecond;
static void finish(void) {
    [realHost shutdown];
    fprintf(stdout,"native playback: %lu checks, %lu projections; %lu failures\n",(unsigned long)observed,(unsigned long)projections,(unsigned long)failures);
    exit(failures?1:0);
}
static void sample(NSUInteger index) {
    [realWindow poll];
    [realWindow.webView evaluateJavaScript:@"({origin:location.origin,hidden:document.hidden,stage:window.fixtureStage,position:window.fixtureAudio?.currentTime||0,paused:window.fixtureAudio?.paused,error:window.fixtureAudio?.error?.code||0,projection:window.__qingyuQQMusic?.projection()})"
        completionHandler:^(id value,NSError *error){
        observed++;check(!error && [value isKindOfClass:NSDictionary.class],"real WebKit returns playback state");
        if([value isKindOfClass:NSDictionary.class]){
            NSString *stage=value[@"stage"];double position=[value[@"position"] doubleValue];
            if([stage isEqual:@"gap"]){observedGap=YES;check(![value[@"projection"][@"media"][@"playing"] boolValue],"real paused audio overrides a stale pause button");}
            if([stage isEqual:@"second"]){
                if(observedSecond)check(position>previousPosition+0.4,"hidden audio currentTime continues advancing");
                observedSecond=YES;previousPosition=position;startedSamples++;
                check(![value[@"paused"] boolValue] && [value[@"error"] integerValue]==0,"hidden media remains playable");
            }
            check(!realWindow.window.visible && !realWindow.disposed,"closed player remains retained in the background");
        }
        if(index==18){
            check(observedGap,"observed a real track gap");check(observedSecond && startedSamples>=10 && previousPosition>10,"real media advances beyond bridge freshness deadline");
            check(lastProjection!=nil && [lastProjection[@"media"][@"playing"] boolValue],"native bridge retains confirmed playing state");finish();return;
        }
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW,NSEC_PER_SEC),dispatch_get_main_queue(),^{sample(index+1);});
    }];
}
int main(void) {@autoreleasepool{
    [NSApplication sharedApplication];[NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
    testSlowPoll();testPlayerOwnership();realHost=host();realWindow=[realHost create:1 configuration:[WKWebViewConfiguration new]];
    realWindow.audioSelection=[realHost beginAudioSelection];
    NSString *audio=silentWAV();
    NSString *html=[NSString stringWithFormat:@"<html><body><div class='song_info__name'><a href='/n/ryqq/songDetail/fixture'>Native fixture</a></div><a class='btn_big_play btn_big_play--pause'>暂停</a><script>window.fixtureStage='first';window.fixtureAudio=new Audio('%@');fixtureAudio.play();setTimeout(()=>{fixtureAudio.pause();window.fixtureStage='gap';setTimeout(()=>{window.fixtureAudio=new Audio('%@');window.fixtureStage='second';fixtureAudio.play();},1500);},3000);</script></body></html>",audio,audio];
    [realWindow.webView loadHTMLString:html baseURL:[NSURL URLWithString:QYQQPlayerURL]];
    // The same close path the real player uses must preserve its WKWebView.
    [realWindow windowShouldClose:realWindow.window];
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,NSEC_PER_SEC),dispatch_get_main_queue(),^{sample(0);});
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW,30*NSEC_PER_SEC),dispatch_get_main_queue(),^{check(NO,"native playback probe timed out");finish();});
    [NSApp run];
}}
