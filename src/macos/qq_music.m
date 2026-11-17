// Official QQ Music website host. Every WebKit/AppKit object is confined to the
// main thread; the Rust media worker receives bounded snapshots, not a web view.
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdatomic.h>
#include <stdbool.h>
#include <math.h>

extern void qy_qq_media_reset(uint64_t view, uint64_t epoch);
extern void qy_qq_media_remove(uint64_t view);
extern void qy_qq_media_update(uint64_t view, uint64_t epoch, const uint8_t *bytes, size_t length);
extern void qy_qq_projection_update(uint64_t view,uint64_t epoch,const uint8_t *bytes,size_t length);
extern void qy_qq_library_status(const char *message);
extern void qy_qq_restoration_failed(const char *message);
extern void qy_qq_cache_clear(void);
extern void qy_qq_playing_result(uint64_t request,bool success);
extern void qy_qq_start_result(uint64_t request,bool success,int32_t seek,const char *message);

static NSString *const QYQQLibraryURL = @"https://y.qq.com/n/ryqq_v2/profile";
static NSString *const QYQQPlayerURL = @"https://y.qq.com/n/ryqq/player";
static NSString *const QYQQHandler = @"qingyuQQMusic";
static uint64_t nextView = 1;
static _Atomic(bool) commandPending = false;

static BOOL qy_vendor_host(NSString *host) {
    host = host.lowercaseString;
    return [host isEqualToString:@"qq.com"] || [host hasSuffix:@".qq.com"] ||
           [host isEqualToString:@"tencent.com"] || [host hasSuffix:@".tencent.com"];
}
static BOOL qy_navigation_url(NSURL *url) {
    return [url.scheme.lowercaseString isEqualToString:@"https"] && qy_vendor_host(url.host) &&
        (!url.port || url.port.integerValue == 443) && !url.user.length && !url.password.length;
}
static BOOL qy_media_url(NSURL *url) {
    return qy_navigation_url(url) && [url.host.lowercaseString isEqualToString:@"y.qq.com"];
}
static BOOL qy_blank_url(NSURL *url) { return [url.absoluteString isEqualToString:@"about:blank"]; }
static BOOL qy_frame_is_vendor(WKFrameInfo *frame) {
    WKSecurityOrigin *origin = frame.securityOrigin;
    return [origin.protocol isEqualToString:@"https"] && qy_vendor_host(origin.host) && (origin.port == 0 || origin.port == 443);
}
static BOOL qy_projection_bounded(id value,NSUInteger depth,NSUInteger *budget){
    if(depth>5 || *budget>48000)return NO;
    if(value==NSNull.null || [value isKindOfClass:NSNumber.class]){*budget+=8;return YES;}
    if([value isKindOfClass:NSString.class]){NSUInteger count=[value length];if(count>4096)return NO;*budget+=count;return *budget<=48000;}
    if([value isKindOfClass:NSArray.class]){if([value count]>256)return NO;for(id item in value)if(!qy_projection_bounded(item,depth+1,budget))return NO;return YES;}
    if([value isKindOfClass:NSDictionary.class]){if([value count]>16)return NO;for(id key in value){if(![key isKindOfClass:NSString.class] || [key length]>128 || !qy_projection_bounded(value[key],depth+1,budget))return NO;}return YES;}
    return NO;
}

@class QYQQWindow, QYQQPlaybackRequest, QYQQResumeRequest;
// WebKit can ask a named popup to focus itself. Only an explicit Qingyu
// "show" action may present these windows; background playlist work stays out
// of the user's window stack even when the site's script calls window.focus().
@interface QYQQNativeWindow : NSWindow
@property BOOL presentationAllowed;
@end
@implementation QYQQNativeWindow
- (BOOL)canBecomeKeyWindow { return self.presentationAllowed && [super canBecomeKeyWindow]; }
- (BOOL)canBecomeMainWindow { return self.presentationAllowed && [super canBecomeMainWindow]; }
- (void)makeKeyAndOrderFront:(id)sender { if(self.presentationAllowed)[super makeKeyAndOrderFront:sender]; }
- (void)orderFront:(id)sender { if(self.presentationAllowed)[super orderFront:sender]; }
- (void)orderFrontRegardless { if(self.presentationAllowed)[super orderFrontRegardless]; }
- (void)orderWindow:(NSWindowOrderingMode)place relativeTo:(NSInteger)otherWindowNumber {
    if(place!=NSWindowOut && !self.presentationAllowed)return;
    [super orderWindow:place relativeTo:otherWindowNumber];
}
@end
@interface QYQQCoordinator : NSObject
@property(strong) NSMutableArray<QYQQWindow *> *windows;
@property(strong) WKWebsiteDataStore *store;
@property(copy) NSString *script;
@property(copy) NSString *profile;
@property(strong) dispatch_source_t pulse;
@property BOOL temporaryProfile;
@property(strong) QYQQWindow *libraryWorker;
@property(strong) NSArray<NSURL *> *libraryQueue;
@property NSUInteger libraryIndex;
@property double libraryDeadline;
@property(copy) NSString *playlistTarget;
@property double playlistDeadline;
@property BOOL playlistSubmitted;
@property BOOL actionBusy;
@property double workerReadyAt;
@property BOOL backgroundPlayback;
@property(strong) QYQQPlaybackRequest *activePlayback;
@property(strong) QYQQPlaybackRequest *queuedPlayback;
@property(strong) dispatch_source_t playbackPulse;
@property(strong) QYQQResumeRequest *resumeRequest;
@property BOOL resumeBusy;
@property(strong) QYQQWindow *audioOwner,*audioCandidate;
@property uint64_t audioSelection,audioLease;
@property BOOL audioBusy;
@property BOOL audioOwnerEstablished;
- (QYQQWindow *)create:(NSInteger)role configuration:(WKWebViewConfiguration *)configuration;
- (BOOL)open:(NSInteger)page;
- (void)discard:(QYQQWindow *)window;
- (void)shutdown;
- (BOOL)refreshLibrary;
- (BOOL)playPlaylist:(NSString *)url;
- (void)advanceLibrary;
- (void)releaseLibraryWorker;
- (QYQQWindow *)player;
- (void)advancePlayback;
- (void)queuePlayback:(QYQQPlaybackRequest *)request;
- (BOOL)startResume:(NSDictionary *)descriptor identifier:(uint64_t)identifier;
- (void)advanceResume;
- (void)finishResume:(BOOL)success seek:(int32_t)seek message:(NSString *)message;
- (BOOL)sameSong:(NSString *)first other:(NSString *)second;
- (void)loadResumeWorker:(QYQQResumeRequest *)request playlist:(BOOL)playlist;
- (uint64_t)beginAudioSelection;
- (void)requestAudio:(QYQQWindow *)window;
- (void)advanceAudio;
- (void)pauseAudio:(QYQQWindow *)window;
- (void)audioDidPlay:(QYQQWindow *)window;
@end

@interface QYQQResumeRequest : NSObject
@property uint64_t identifier;
@property(copy) NSString *track,*playlist;
@property double position,duration,deadline;
@property NSInteger stage;
@property BOOL requested,preservePosition,createdPlayer;
@property uint64_t workerEpoch;
@property uint64_t baselineEpoch;
@property uint64_t baselineViewID;
@property(copy) NSString *baselineTrack;
@property uint64_t audioSelection;
@property uint64_t initialOwnerID;
@property BOOL initialOwnerPlaying;
@end
@implementation QYQQResumeRequest
@end
static QYQQCoordinator *coordinator;

@interface QYQQWindow : NSWindowController <NSWindowDelegate, WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler, NSToolbarDelegate>
@property(weak) QYQQCoordinator *host;
@property(strong) WKWebView *webView;
@property(strong) NSTextField *statusLabel;
@property(strong) NSButton *retryButton;
@property(strong) NSProgressIndicator *spinner;
@property(strong) NSURL *retryURL;
@property uint64_t viewID;
@property uint64_t epoch;
@property NSInteger role;
@property BOOL ready;
@property BOOL disposed;
@property BOOL hasMedia;
@property BOOL playing;
@property BOOL pollBusy;
@property BOOL commandBusy;
@property BOOL alertVisible;
@property double pollStarted;
@property double lastMessageAt;
@property double lastProjectionAt;
@property double lastDiagnosticAt;
@property BOOL diagnosticBusy;
@property(copy) NSString *documentToken;
@property(copy) NSString *trackKey;
@property(copy) NSString *trackURL;
@property uint64_t projectionSequence;
@property uint64_t audioSelection,pendingAudioRequest;
@property(copy) NSString *pendingAudioDocument;
@property BOOL pendingAudioUser,audioGateReady,audioSuspended,audioPausedByUser,audioRetired;
@property BOOL audioProbeBusy;
- (instancetype)initWithOwner:(QYQQCoordinator *)owner role:(NSInteger)role configuration:(WKWebViewConfiguration *)configuration;
- (void)poll;
- (void)diagnostics;
- (void)acceptSnapshot:(id)value;
- (void)invalidateDocument;
- (void)tearDown;
- (void)show;
- (void)revokeAudio:(uint64_t)lease;
- (void)probeAudio;
@end

@interface QYQQPlaybackRequest : NSObject
@property uint64_t requestID,viewID,epoch;
@property(copy) NSString *document,*track;
@property BOOL playing,notified;
@property double expires,executionDeadline;
- (void)finish:(BOOL)success;
@end
@implementation QYQQPlaybackRequest
- (void)finish:(BOOL)success {
    if(self.notified)return;self.notified=YES;qy_qq_playing_result(self.requestID,success);
}
@end

@implementation QYQQCoordinator
- (QYQQWindow *)player {
    if(self.audioOwner && !self.audioOwner.disposed)return self.audioOwner;
    if(self.audioCandidate && self.audioCandidate.role==1 && !self.audioCandidate.disposed)return self.audioCandidate;
    for(QYQQWindow *window in self.windows)if(window.role==1 && !window.disposed)return window;
    return nil;
}
- (uint64_t)beginAudioSelection {
    return [self beginAudioSelectionFor:nil];
}
- (uint64_t)beginAudioSelectionFor:(QYQQWindow *)kept {
    self.audioSelection++;self.audioCandidate=nil;
    for(QYQQWindow *window in self.windows){
        if(window==kept)continue;
        if(window!=self.audioOwner || self.audioBusy)[window revokeAudio:++self.audioLease];
        window.pendingAudioDocument=nil;window.pendingAudioRequest=0;
    }
    return self.audioSelection;
}
- (BOOL)audioEligible:(QYQQWindow *)window {
    return window && !window.disposed && window.ready && window.audioGateReady && !window.audioPausedByUser &&
        qy_media_url(window.webView.URL) && window.pendingAudioRequest &&
        [window.documentToken isEqualToString:window.pendingAudioDocument] &&
        ((window==self.audioOwner && self.audioOwnerEstablished) || ((window.role==1 || window==self.libraryWorker || window.window.visible) &&
            window.audioSelection && window.audioSelection==self.audioSelection));
}
- (void)requestAudio:(QYQQWindow *)window {
    if(!window || window.disposed || !window.ready || !window.audioGateReady || !window.documentToken)return;
    if(![window.documentToken isEqualToString:window.pendingAudioDocument]){
        window.pendingAudioDocument=nil;window.pendingAudioRequest=0;return;
    }
    if(window.pendingAudioUser && window.window.visible){
        // Only a real website user activation may revive a paused/retired page.
        uint64_t request=window.pendingAudioRequest;NSString *document=window.pendingAudioDocument;
        window.audioSelection=[self beginAudioSelectionFor:window];window.pendingAudioRequest=request;window.pendingAudioDocument=document;
        window.audioPausedByUser=NO;window.audioRetired=NO;
    }
    window.pendingAudioUser=NO;
    if(![self audioEligible:window]){[window revokeAudio:++self.audioLease];return;}
    if(window==self.libraryWorker){window.role=1;window.window.title=@"QQ 音乐播放器 · 轻屿";}
    self.audioCandidate=window;[self advanceAudio];
}
- (void)grantAudio:(QYQQWindow *)window {
    if(![self audioEligible:window]){self.audioBusy=NO;[self advanceAudio];return;}
    uint64_t epoch=window.epoch,request=window.pendingAudioRequest,lease=++self.audioLease;
    NSString *document=window.pendingAudioDocument;
    __weak QYQQCoordinator *weak=self;__weak QYQQWindow *weakWindow=window;
    [window.webView callAsyncJavaScript:@"const b=window.__qingyuQQMusic;return b?.projection()?.document===document && b.setAudioLease(lease,true,request)===true;"
        arguments:@{@"lease":@(lease),@"request":@(request),@"document":document} inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:^(id result,NSError *error){
        QYQQCoordinator *host=weak;QYQQWindow *target=weakWindow;if(!host)return;
        BOOL current=target && !target.disposed && target.epoch==epoch && [target.documentToken isEqual:document];
        BOOL accepted=current && host.audioOwner==target && host.audioCandidate==target && target.pendingAudioRequest==request &&
            [host audioEligible:target] && !error && [result isKindOfClass:NSNumber.class] && [result boolValue];
        if(accepted && target.pendingAudioRequest==request){target.pendingAudioRequest=0;target.pendingAudioDocument=nil;}
        if(accepted)host.audioOwnerEstablished=YES;
        if(host.audioCandidate==target && accepted)host.audioCandidate=nil;
        host.audioBusy=NO;
        if(!accepted && target.pendingAudioRequest==request)[target revokeAudio:++host.audioLease];
        if(!accepted && host.audioOwner==target && (!current || !target.pendingAudioRequest))[host pauseAudio:target];
        [host advanceAudio];[host advanceResume];
    }];
}
- (void)activateAudioCandidate {
    QYQQWindow *target=self.audioCandidate;
    if(![self audioEligible:target]){self.audioCandidate=nil;self.audioBusy=NO;return;}
    self.audioOwner=target;self.audioOwnerEstablished=NO;uint64_t epoch=target.epoch;
    __weak QYQQCoordinator *weak=self;__weak QYQQWindow *weakWindow=target;
    [target.webView setAllMediaPlaybackSuspended:NO completionHandler:^{
        QYQQCoordinator *host=weak;QYQQWindow *window=weakWindow;if(!host)return;
        window.audioSuspended=NO;
        if(window && !window.disposed && window.epoch==epoch && host.audioOwner==window && host.audioCandidate==window && [host audioEligible:window]){
            window.audioRetired=NO;[host grantAudio:window];return;
        }
        // An intervening selection/navigation cannot complete an older grant.
        [window revokeAudio:++host.audioLease];
        WKWebView *view=window.webView;
        [view setAllMediaPlaybackSuspended:YES completionHandler:^{
            QYQQWindow *current=weakWindow;
            current.audioSuspended=YES;if(host.audioOwner==current)host.audioOwner=nil;
            host.audioBusy=NO;[host advanceAudio];
        }];
        if(!window || window.disposed){if(host.audioOwner==window)host.audioOwner=nil;host.audioBusy=NO;[host advanceAudio];}
    }];
}
- (void)advanceAudio {
    if(self.audioBusy)return;
    QYQQWindow *candidate=self.audioCandidate,*previous=self.audioOwner;
    if(candidate && ![self audioEligible:candidate]){self.audioCandidate=nil;candidate=nil;}
    if(!candidate && (!previous || (!previous.disposed && !previous.audioPausedByUser)))return;
    self.audioBusy=YES;
    if(candidate==previous && !previous.audioSuspended){[self grantAudio:previous];return;}
    self.audioOwner=nil;self.audioOwnerEstablished=NO;
    if(previous && !previous.disposed){
        previous.audioRetired=YES;[previous revokeAudio:++self.audioLease];
        __weak QYQQCoordinator *weak=self;
        // WebKit's public suspension gate also covers frames and late site
        // callbacks. Do not un-suspend the next view before this completion.
        WKWebView *previousView=previous.webView;
        [previousView pauseAllMediaPlaybackWithCompletionHandler:^{
            [previousView setAllMediaPlaybackSuspended:YES completionHandler:^{
                previous.audioSuspended=YES;[weak activateAudioCandidate];
            }];
        }];
    }else [self activateAudioCandidate];
}
- (void)pauseAudio:(QYQQWindow *)window {
    window.audioPausedByUser=YES;window.pendingAudioRequest=0;window.pendingAudioDocument=nil;
    [window revokeAudio:++self.audioLease];
    if(self.audioCandidate==window)self.audioCandidate=nil;
    [self advanceAudio];
}
- (void)audioDidPlay:(QYQQWindow *)window {
    if(window!=self.audioOwner || !window.playing)return;
    if(self.playlistSubmitted && window.audioSelection==self.audioSelection){
        self.playlistTarget=nil;self.playlistSubmitted=NO;qy_qq_library_status("");
        if(!self.resumeRequest && !self.libraryQueue)[self releaseLibraryWorker];
    }
    for(QYQQWindow *old in self.windows.copy)if(old!=window && old!=self.audioCandidate && old.audioSuspended && !old.window.visible && old!=self.libraryWorker &&
        (old.audioRetired || (old.role==1 && old.audioSelection<window.audioSelection)))[self discard:old];
}
- (QYQQWindow *)create:(NSInteger)role configuration:(WKWebViewConfiguration *)configuration {
    // Keep the library, player, and at most two account/pop-up windows. Hidden
    // account windows without media can be retired before opening another one.
    if (self.windows.count >= 4) {
        QYQQWindow *retired = nil;
        for (QYQQWindow *candidate in self.windows) {
            if (candidate!=self.audioOwner && candidate!=self.audioCandidate && candidate!=self.libraryWorker && !candidate.window.visible &&
                ((candidate.role == 2 && !candidate.hasMedia) || candidate.audioRetired)) { retired = candidate; break; }
        }
        if (retired) [self discard:retired];
    }
    if (self.windows.count >= 4) return nil;
    QYQQWindow *window = [[QYQQWindow alloc] initWithOwner:self role:role configuration:configuration];
    if (window) [self.windows addObject:window];
    return window;
}
- (BOOL)open:(NSInteger)page {
    QYQQWindow *existing = nil;
    NSInteger best = -1;
    for (QYQQWindow *window in self.windows) {
        NSInteger priority = (window.playing ? 4 : 0) + (window.hasMedia ? 2 : 0) + (window.window.visible ? 1 : 0);
        if (window.role == page && priority >= best) { existing = window; best = priority; }
    }
    if (existing) { [existing show]; return YES; }
    WKWebViewConfiguration *configuration = [WKWebViewConfiguration new];
    configuration.websiteDataStore = self.store;
    QYQQWindow *window = [self create:page configuration:configuration];
    if (!window) return NO;
    if(page==1)window.audioSelection=[self beginAudioSelection];
    [window.webView loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:page == 1 ? QYQQPlayerURL : QYQQLibraryURL]]];
    [window show]; return YES;
}
- (void)discard:(QYQQWindow *)window {
    if(window==self.libraryWorker)self.libraryWorker=nil;
    if(window==self.audioOwner)[self pauseAudio:window];
    if(window==self.audioCandidate)self.audioCandidate=nil;
    [window tearDown];
    [self.windows removeObjectIdenticalTo:window];
}
- (void)shutdown {
    [self beginAudioSelection];
    [self finishResume:NO seek:0 message:@"已停止恢复 QQ 音乐"];
    [self.activePlayback finish:NO];[self.queuedPlayback finish:NO];self.activePlayback=nil;self.queuedPlayback=nil;
    if(self.playbackPulse)dispatch_source_cancel(self.playbackPulse);self.playbackPulse=nil;
    if (self.pulse) dispatch_source_cancel(self.pulse); self.pulse = nil;
    for (QYQQWindow *window in self.windows.copy) [window tearDown];
    [self.windows removeAllObjects];
    self.audioOwner=nil;self.audioCandidate=nil;self.audioBusy=NO;self.audioOwnerEstablished=NO;
    self.libraryWorker=nil;self.libraryQueue=nil;self.playlistTarget=nil;
    qy_qq_cache_clear();
}
- (void)finishResume:(BOOL)success seek:(int32_t)seek message:(NSString *)message {
    QYQQResumeRequest *request=self.resumeRequest;if(!request)return;
    self.resumeRequest=nil;
    self.resumeBusy=NO;
    if(!success){
        QYQQWindow *owner=self.audioOwner;
        if(owner.audioSelection==request.audioSelection && (!request.initialOwnerPlaying || owner.viewID!=request.initialOwnerID ||
            ![owner.trackKey isEqualToString:request.baselineTrack]))[self pauseAudio:owner];
        if(request.audioSelection==self.audioSelection)[self beginAudioSelection];
        for(QYQQWindow *window in self.windows)[window.webView callAsyncJavaScript:@"window.__qingyuQQMusic?.cancelResume(request);"
            arguments:@{@"request":@(request.identifier)} inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:nil];
    }
    qy_qq_start_result(request.identifier,success,seek,(message?:@"").UTF8String);
    if(self.libraryWorker && !self.libraryQueue && !self.playlistTarget)[self releaseLibraryWorker];
    if(!success && request.createdPlayer){QYQQWindow *player=[self player];if(player && !player.playing)[self discard:player];}
}
- (BOOL)startResume:(NSDictionary *)descriptor identifier:(uint64_t)identifier {
    NSString *track=descriptor[@"track_url"],*playlist=descriptor[@"playlist_url"];
    if(![track isKindOfClass:NSString.class] || ![playlist isKindOfClass:NSString.class] || track.length>2048 || playlist.length>2048)return NO;
    NSRegularExpression *song=[NSRegularExpression regularExpressionWithPattern:@"^/n/ryqq(?:_v2)?/songDetail/[a-zA-Z0-9]{1,64}$" options:0 error:nil];
    NSRegularExpression *list=[NSRegularExpression regularExpressionWithPattern:@"^/n/ryqq(?:_v2)?/(?:playlist/[0-9]{1,20}|profile/like/song)$" options:0 error:nil];
    for(NSString *value in @[track,playlist]){
        if(!value.length)continue;NSURL *url=[NSURL URLWithString:value];
        NSRegularExpression *pattern=[value isEqualToString:track]&&track.length?song:list;
        if(!qy_media_url(url) || url.query.length || url.fragment.length || [pattern numberOfMatchesInString:url.path options:0 range:NSMakeRange(0,url.path.length)]!=1)return NO;
    }
    if(!track.length && !playlist.length)return NO;
    if(![descriptor[@"position"] isKindOfClass:NSNumber.class] || ![descriptor[@"duration"] isKindOfClass:NSNumber.class])return NO;
    double position=[descriptor[@"position"] doubleValue],duration=[descriptor[@"duration"] doubleValue];
    if(!isfinite(position)||!isfinite(duration)||position<0||duration<0||position>604800||duration>604800)return NO;
    [self finishResume:NO seek:0 message:@"已改为新的播放选择"];
    self.libraryQueue=nil;self.playlistTarget=nil;[self releaseLibraryWorker];
    QYQQResumeRequest *request=[QYQQResumeRequest new];request.identifier=identifier;request.track=track;request.playlist=playlist;
    request.initialOwnerID=self.audioOwner.viewID;request.initialOwnerPlaying=self.audioOwner.playing;
    request.audioSelection=[self beginAudioSelection];
    request.position=position;request.duration=duration;request.deadline=NSProcessInfo.processInfo.systemUptime+20;
    request.stage=0;self.resumeRequest=request;self.backgroundPlayback=YES;
    QYQQWindow *player=[self player];request.preservePosition=player.ready && track.length && [self sameSong:player.trackURL other:track];
    request.baselineEpoch=player.epoch;request.baselineViewID=player.viewID;request.baselineTrack=player.trackKey?:@"";
    if(!player){WKWebViewConfiguration *configuration=[WKWebViewConfiguration new];configuration.websiteDataStore=self.store;
        player=[self create:1 configuration:configuration];request.createdPlayer=YES;
        if(!player){[self finishResume:NO seek:0 message:@"无法创建 QQ 音乐播放器"];return YES;}
        [player.webView loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:QYQQPlayerURL]]];
    }
    if(track.length || request.createdPlayer){player.audioSelection=request.audioSelection;player.audioPausedByUser=NO;}
    ((QYQQNativeWindow *)player.window).presentationAllowed=NO;[player.window orderOut:nil];
    [self advanceResume];return YES;
}
- (void)advanceResume {
    QYQQResumeRequest *request=self.resumeRequest;if(!request)return;
    if(NSProcessInfo.processInfo.systemUptime>request.deadline){[self finishResume:NO seek:0 message:@"QQ 音乐恢复超时，请检查网络后重试"];return;}
    if(self.resumeBusy)return;
    QYQQWindow *player=[self player];
    QYQQWindow *target=player;
    if(request.stage==1 || request.stage==2)target=self.libraryWorker;
    if(!target || !target.ready || target.disposed || !qy_media_url(target.webView.URL))return;
    if(request.stage==3 && !request.track.length && player.viewID==request.baselineViewID && player.epoch==request.baselineEpoch && [player.trackKey isEqualToString:request.baselineTrack])return;
    if(!request.track.length && request.stage==0){[self loadResumeWorker:request playlist:YES];return;}
    self.resumeBusy=YES;uint64_t epoch=target.epoch;
    double remaining=MAX(0,request.deadline-NSProcessInfo.processInfo.systemUptime);
    NSDictionary *arguments=@{@"request":@(request.identifier),@"track_url":request.track,@"playlist_url":request.playlist,
        @"position":@(request.position),@"preserve_position":@(request.preservePosition),@"accept_any":@(request.requested),
        @"deadline":@(NSDate.date.timeIntervalSince1970*1000.0+remaining*1000.0)};
    __weak QYQQCoordinator *weak=self;__weak QYQQWindow *weakWindow=target;
    [target.webView callAsyncJavaScript:@"if(!window.__qingyuQQMusic)return null;return await window.__qingyuQQMusic.resumeStep(intent);"
        arguments:@{@"intent":arguments} inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:^(id result,NSError *error){
        QYQQCoordinator *host=weak;QYQQWindow *window=weakWindow;
        if(!host || host.resumeRequest!=request)return;
        host.resumeBusy=NO;
        if(!window || window.disposed || window.epoch!=epoch)return;
        if(NSProcessInfo.processInfo.systemUptime>request.deadline){[host finishResume:NO seek:0 message:@"QQ 音乐恢复超时，请重试"];return;}
        if(error || ![result isKindOfClass:NSDictionary.class])return;
        NSString *status=result[@"status"];
        if([status isEqual:@"expired"]){[host finishResume:NO seek:-2 message:@"QQ 音乐官方登录已失效，请重新登录"];return;}
        if([result[@"projection"] isKindOfClass:NSDictionary.class])[window acceptSnapshot:result[@"projection"]];
        if([status isEqual:@"playing"] && window==host.audioOwner && window.playing){
            NSDictionary *projection=[result[@"projection"] isKindOfClass:NSDictionary.class]?result[@"projection"]:nil;
            NSDictionary *media=[projection[@"media"] isKindOfClass:NSDictionary.class]?projection[@"media"]:nil;
            if(!media || ![media[@"playing"] isKindOfClass:NSNumber.class] || ![media[@"playing"] boolValue])return;
            NSString *actual=[media[@"track_url"] isKindOfClass:NSString.class]?media[@"track_url"]:@"";
            if(request.track.length && ![self sameSong:actual other:request.track])return;
            [host finishResume:YES seek:[result[@"seek"] isKindOfClass:NSNumber.class]?[result[@"seek"] intValue]:-1 message:@""];return;
        }
        if([status isEqual:@"requested"]){
            request.requested=YES;request.stage=3;
            dispatch_async(dispatch_get_main_queue(),^{[host advanceResume];});return;
        }
        if([status isEqual:@"missing"]){
            if(request.stage==0){[host loadResumeWorker:request playlist:request.playlist.length>0];return;}
            if(request.stage==1 && request.track.length){[host loadResumeWorker:request playlist:NO];return;}
        }
        if([status isEqual:@"invalid"]||[status isEqual:@"stale"])[host finishResume:NO seek:0 message:@"QQ 音乐播放记录已变化，请重新选择歌曲"];
    }];
}
- (BOOL)sameSong:(NSString *)first other:(NSString *)second {
    if(!first.length || !second.length)return NO;
    NSURL *a=[NSURL URLWithString:first],*b=[NSURL URLWithString:second];
    return qy_media_url(a)&&qy_media_url(b)&&[a.path.lastPathComponent isEqualToString:b.path.lastPathComponent];
}
- (void)loadResumeWorker:(QYQQResumeRequest *)request playlist:(BOOL)playlist {
    if(self.resumeRequest!=request)return;
    NSString *url=playlist?request.playlist:request.track;
    if(!url.length){[self finishResume:NO seek:0 message:@"没有可恢复的歌曲，请从歌单选择音乐"];return;}
    QYQQWindow *worker=[self hiddenLibrary];if(!worker){[self finishResume:NO seek:0 message:@"无法读取 QQ 音乐播放记录"];return;}
    worker.audioSelection=request.audioSelection;worker.audioPausedByUser=NO;
    request.stage=playlist?1:2;request.requested=NO;
    [worker.webView loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:url]]];
}
- (BOOL)matchesPlayback:(QYQQPlaybackRequest *)request window:(QYQQWindow *)window {
    return window && !window.disposed && window.ready && window.viewID==request.viewID && window.epoch==request.epoch &&
        qy_media_url(window.webView.URL) && [window.documentToken isEqualToString:request.document] && [window.trackKey isEqualToString:request.track];
}
- (void)queuePlayback:(QYQQPlaybackRequest *)request {
    for(QYQQWindow *window in self.windows)if([self matchesPlayback:request window:window]){
        if(request.playing){window.audioSelection=[self beginAudioSelectionFor:window];window.audioPausedByUser=NO;}
        else {[self finishResume:NO seek:0 message:@"已暂停 QQ 音乐"];self.playlistTarget=nil;self.playlistSubmitted=NO;[self pauseAudio:window];}
        break;
    }
    // Constant pending work: one WebKit evaluation plus only the latest target.
    [self.queuedPlayback finish:NO];self.queuedPlayback=request;
    if(!self.playbackPulse){
        self.playbackPulse=dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER,0,0,dispatch_get_main_queue());
        dispatch_source_set_timer(self.playbackPulse,dispatch_time(DISPATCH_TIME_NOW,20*NSEC_PER_MSEC),20*NSEC_PER_MSEC,2*NSEC_PER_MSEC);
        __weak QYQQCoordinator *weak=self;dispatch_source_set_event_handler(self.playbackPulse,^{[weak advancePlayback];});dispatch_resume(self.playbackPulse);
    }
    [self advancePlayback];
}
- (void)advancePlayback {
    double now=NSProcessInfo.processInfo.systemUptime;
    if(self.activePlayback && now>self.activePlayback.executionDeadline)[self.activePlayback finish:NO];
    if(self.queuedPlayback && now>self.queuedPlayback.expires){[self.queuedPlayback finish:NO];self.queuedPlayback=nil;}
    if(!self.activePlayback && self.queuedPlayback){
        QYQQPlaybackRequest *request=self.queuedPlayback;QYQQWindow *target=nil;
        for(QYQQWindow *window in self.windows)if(window.viewID==request.viewID){target=window;break;}
        if(![self matchesPlayback:request window:target]){[request finish:NO];self.queuedPlayback=nil;}
        else if(!target.commandBusy){
            self.queuedPlayback=nil;self.activePlayback=request;target.commandBusy=YES;
            request.executionDeadline=MIN(request.expires,now+0.6);
            double deadline=NSDate.date.timeIntervalSince1970*1000.0+MAX(0,request.executionDeadline-now)*1000.0;
            NSDictionary *arguments=@{@"document":request.document,@"track":request.track,@"playing":@(request.playing),@"deadline":@(deadline)};
            __weak QYQQCoordinator *weak=self;__weak QYQQWindow *weakWindow=target;
            [target.webView callAsyncJavaScript:@"const b=window.__qingyuQQMusic;if(!b)return null;const confirmed=await b.command({operation:6,value:playing?1:0,document,track_key:track,deadline});return {confirmed,projection:b.projection()};"
                arguments:arguments inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:^(id result,NSError *error){
                QYQQCoordinator *host=weak;QYQQWindow *window=weakWindow;
                if(!host || host.activePlayback!=request){[request finish:NO];return;}
                BOOL valid=[host matchesPlayback:request window:window] && !error && [result isKindOfClass:NSDictionary.class] &&
                    [result[@"confirmed"] isKindOfClass:NSNumber.class] && [result[@"confirmed"] boolValue] &&
                    NSProcessInfo.processInfo.systemUptime<=request.executionDeadline;
                if(valid){
                    NSDictionary *projection=result[@"projection"];
                    NSDictionary *media=[projection isKindOfClass:NSDictionary.class]&&[projection[@"media"] isKindOfClass:NSDictionary.class]?projection[@"media"]:nil;
                    valid=media && [projection[@"document"] isEqualToString:request.document] && [media[@"track_key"] isEqualToString:request.track] &&
                        [media[@"playing"] isKindOfClass:NSNumber.class] && [media[@"playing"] boolValue]==request.playing;
                    if(valid){[window acceptSnapshot:projection];valid=window.playing==request.playing && [window.trackKey isEqualToString:request.track];
                        if(valid && !request.playing)[host pauseAudio:window];}
                }
                if(window && window.epoch==request.epoch)window.commandBusy=NO;
                [request finish:valid];host.activePlayback=nil;[host advancePlayback];
            }];
        }
    }
    // A hung WebKit callback keeps its one in-flight slot, but consumes no fast
    // timer after all receipts expire. Newest queued work has its own deadline.
    if(!self.queuedPlayback && (!self.activePlayback || self.activePlayback.notified)){
        if(self.playbackPulse)dispatch_source_cancel(self.playbackPulse);self.playbackPulse=nil;
    }
}
- (QYQQWindow *)hiddenLibrary {
    if (self.libraryWorker && !self.libraryWorker.disposed) {
        // Official navigation can turn the temporary page into a player.
        // Retain that page for playback and give enumeration a fresh worker.
        if(self.libraryWorker==self.audioOwner || self.libraryWorker==self.audioCandidate)[self releaseLibraryWorker];
        else return self.libraryWorker;
    }
    WKWebViewConfiguration *configuration=[WKWebViewConfiguration new];configuration.websiteDataStore=self.store;
    self.libraryWorker=[self create:3 configuration:configuration];
    return self.libraryWorker;
}
- (BOOL)refreshLibrary {
    [self finishResume:NO seek:0 message:@"已取消恢复播放，正在刷新歌单"];
    QYQQWindow *worker=[self hiddenLibrary];if(!worker)return NO;
    worker.audioSelection=0;[worker revokeAudio:++self.audioLease];
    self.playlistTarget=nil;self.actionBusy=NO;
    self.libraryQueue=@[[NSURL URLWithString:QYQQLibraryURL]];self.libraryIndex=0;
    self.libraryDeadline=NSProcessInfo.processInfo.systemUptime+20;
    [worker.webView loadRequest:[NSURLRequest requestWithURL:self.libraryQueue[0]]];return YES;
}
- (BOOL)playPlaylist:(NSString *)url {
    NSURL *target=[NSURL URLWithString:url];
    if (!qy_media_url(target)) return NO;
    NSRegularExpression *pattern=[NSRegularExpression regularExpressionWithPattern:@"^/n/ryqq(?:_v2)?/(?:playlist/[0-9]{1,20}|profile/like/song)$" options:0 error:nil];
    if([pattern numberOfMatchesInString:target.path options:0 range:NSMakeRange(0,target.path.length)]!=1)return NO;
    QYQQWindow *worker=[self hiddenLibrary];if(!worker)return NO;
    worker.audioSelection=[self beginAudioSelection];worker.audioPausedByUser=NO;
    self.backgroundPlayback=YES;
    QYQQWindow *player=[self player];
    if(player){((QYQQNativeWindow *)player.window).presentationAllowed=NO;[player.window orderOut:nil];}
    self.libraryQueue=nil;self.playlistTarget=target.absoluteString;self.playlistSubmitted=NO;self.actionBusy=NO;
    self.playlistDeadline=NSProcessInfo.processInfo.systemUptime+15;
    [worker.webView loadRequest:[NSURLRequest requestWithURL:target]];return YES;
}
- (void)releaseLibraryWorker {
    QYQQWindow *worker=self.libraryWorker;
    self.libraryWorker=nil;
    if(worker && worker!=self.audioOwner && worker!=self.audioCandidate)[self discard:worker];
}
- (void)advanceLibrary {
    // Public-DOM actions are implemented by the document bridge. Navigation is
    // deliberately separate from the current player so audio stays alive.
    QYQQWindow *worker=self.libraryWorker;
    if(!worker || worker.disposed)return;
    double now=NSProcessInfo.processInfo.systemUptime;
    if(self.playlistTarget && now>self.playlistDeadline){self.playlistTarget=nil;qy_qq_library_status("QQ 音乐歌单播放未完成，请打开官方窗口重试");[self releaseLibraryWorker];return;}
    if(self.libraryQueue && now>self.libraryDeadline){self.libraryQueue=nil;qy_qq_restoration_failed("恢复 QQ 音乐连接超时，账号记录已保留，请重试");[self releaseLibraryWorker];return;}
    if(!worker.ready || self.actionBusy || self.playlistSubmitted || (!self.playlistTarget && !self.libraryQueue) || now-self.workerReadyAt<1)return;
    self.actionBusy=YES;uint64_t epoch=worker.epoch;
    __weak QYQQCoordinator *weak=self;
    if(self.playlistTarget){
        NSString *target=self.playlistTarget;
        [worker.webView callAsyncJavaScript:@"return window.__qingyuQQMusic ? window.__qingyuQQMusic.activatePlaylist(target) : false;"
            arguments:@{@"target":target} inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:^(id result,NSError *error){
            QYQQCoordinator *host=weak;host.actionBusy=NO;
            if(!host || host.libraryWorker.epoch!=epoch || ![host.playlistTarget isEqualToString:target])return;
            if(!error && [result isKindOfClass:NSNumber.class] && [result boolValue]){
                host.playlistSubmitted=YES;qy_qq_library_status("已向 QQ 音乐提交播放，正在等待播放器更新");
            }
        }];return;
    }
    [worker.webView evaluateJavaScript:@"window.__qingyuQQMusic ? ({projection:window.__qingyuQQMusic.projection(),routes:window.__qingyuQQMusic.libraryRoutes()}) : null"
        completionHandler:^(id result,NSError *error){
        QYQQCoordinator *host=weak;host.actionBusy=NO;
        if(!host || !host.libraryQueue || host.libraryWorker.epoch!=epoch || error || ![result isKindOfClass:NSDictionary.class])return;
        NSDictionary *projection=result[@"projection"];NSDictionary *library=projection[@"library"];
        if(![library isKindOfClass:NSDictionary.class] || [library[@"loading"] boolValue])return;
        [host.libraryWorker acceptSnapshot:projection];
        if(![library[@"logged_in"] boolValue]){
            // An explicit official sign-in CTA may expire a session; an empty
            // hydration frame is kept pending until the bounded deadline.
            if(![library[@"auth"] isEqualToString:@"expired"])return;
            host.libraryQueue=nil;qy_qq_library_status("QQ 音乐官方页面要求重新登录");[host releaseLibraryWorker];return;
        }
        if(host.libraryQueue.count==1){
            NSArray *routes=result[@"routes"];NSMutableArray<NSURL *> *queue=[NSMutableArray new];
            NSRegularExpression *pattern=[NSRegularExpression regularExpressionWithPattern:@"^/n/ryqq(?:_v2)?/profile/(?:like/song|like/playlist|create)$" options:0 error:nil];
            for(id route in [routes isKindOfClass:NSArray.class]?routes:@[]){
                if(![route isKindOfClass:NSString.class] || [route length]>2048)continue;
                NSURL *url=[NSURL URLWithString:route];
                if(qy_media_url(url) && !url.query.length && !url.fragment.length &&
                    [pattern numberOfMatchesInString:url.path options:0 range:NSMakeRange(0,url.path.length)]==1 && ![queue containsObject:url]) [queue addObject:url];
                if(queue.count==3)break;
            }
            if(queue.count){host.libraryQueue=queue;host.libraryIndex=0;}
        }
        if(host.libraryIndex+1<host.libraryQueue.count){
            host.libraryIndex++;
            [host.libraryWorker.webView loadRequest:[NSURLRequest requestWithURL:host.libraryQueue[host.libraryIndex]]];
        }else{host.libraryQueue=nil;qy_qq_library_status("");[host releaseLibraryWorker];}
    }];
}
@end

@implementation QYQQWindow
- (instancetype)initWithOwner:(QYQQCoordinator *)owner role:(NSInteger)role configuration:(WKWebViewConfiguration *)configuration {
    NSWindow *window = [[QYQQNativeWindow alloc] initWithContentRect:NSMakeRect(0,0,1120,760)
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable
        backing:NSBackingStoreBuffered defer:NO];
    self = [super initWithWindow:window];
    if (!self) return nil;
    self.host = owner; self.role = role; self.viewID = nextView++; self.epoch = 1;
    window.title = role == 1 ? @"QQ 音乐播放器 · 轻屿" : @"QQ 音乐 · 轻屿";
    window.subtitle = @"QQ 音乐官方网站";
    window.identifier = [NSString stringWithFormat:@"qingyu.qqmusic.%llu",self.viewID];
    window.contentMinSize = NSMakeSize(840,520);
    window.releasedWhenClosed = NO; window.delegate = self;
    window.collectionBehavior = NSWindowCollectionBehaviorFullScreenPrimary;
    window.toolbarStyle = NSWindowToolbarStyleUnifiedCompact;
    NSString *frameName = [NSString stringWithFormat:@"qingyu.qqmusic.%@.%ld",owner.profile,(long)role];
    BOOL restoredFrame = [window setFrameUsingName:frameName]; [window setFrameAutosaveName:frameName];
    if (!restoredFrame) [window center];
    NSToolbar *toolbar = [[NSToolbar alloc] initWithIdentifier:@"qingyu.qqmusic.navigation"];
    toolbar.delegate = self; toolbar.displayMode = NSToolbarDisplayModeIconAndLabel;
    toolbar.allowsUserCustomization = NO; window.toolbar = toolbar;

    // A newly-created popup must use the configuration supplied by WebKit so
    // that official window.opener/OAuth behavior remains intact.
    configuration.websiteDataStore = owner.store;
    // A song selected in the library opens the official popup, and transport
    // actions arrive through the native panel. Both need audio.play() to work
    // across that window/JavaScript boundary; this host never calls play on open.
    configuration.mediaTypesRequiringUserActionForPlayback = WKAudiovisualMediaTypeNone;
    configuration.preferences.javaScriptCanOpenWindowsAutomatically = YES;
    WKUserContentController *content = [WKUserContentController new];
    [content addScriptMessageHandler:self name:QYQQHandler];
    NSString *managedScript=[@"Object.defineProperty(window,'__qingyuQQAudioManaged',{value:true});\n" stringByAppendingString:owner.script];
    [content addUserScript:[[WKUserScript alloc] initWithSource:managedScript injectionTime:WKUserScriptInjectionTimeAtDocumentStart forMainFrameOnly:NO]];
    configuration.userContentController = content;
    self.webView = [[WKWebView alloc] initWithFrame:NSZeroRect configuration:configuration];
    self.webView.navigationDelegate = self; self.webView.UIDelegate = self;
    self.webView.allowsBackForwardNavigationGestures = YES;
    self.webView.translatesAutoresizingMaskIntoConstraints = NO;
    self.webView.customUserAgent = nil; // Use WebKit's default identity.
    self.audioSuspended=YES;
    __weak QYQQWindow *weakWindow=self;
    [self.webView setAllMediaPlaybackSuspended:YES completionHandler:^{
        QYQQWindow *window=weakWindow;if(!window || window.disposed)return;
        window.audioGateReady=YES;[window.host requestAudio:window];
    }];

    NSView *root = [NSView new];
    NSView *banner = [NSView new]; banner.translatesAutoresizingMaskIntoConstraints = NO;
    self.statusLabel = [NSTextField wrappingLabelWithString:@"在 QQ 音乐官方网站登录，查看喜欢和歌单。关闭窗口后音乐可继续播放。"];
    self.statusLabel.font = [NSFont systemFontOfSize:13];
    self.statusLabel.textColor = NSColor.secondaryLabelColor;
    self.statusLabel.translatesAutoresizingMaskIntoConstraints = NO;
    self.retryButton = [NSButton buttonWithTitle:@"重试" target:self action:@selector(reload:)];
    self.retryButton.translatesAutoresizingMaskIntoConstraints = NO; self.retryButton.hidden = YES;
    self.spinner = [[NSProgressIndicator alloc] initWithFrame:NSZeroRect];
    self.spinner.style = NSProgressIndicatorStyleSpinning; self.spinner.controlSize = NSControlSizeSmall;
    self.spinner.displayedWhenStopped = NO; self.spinner.translatesAutoresizingMaskIntoConstraints = NO;
    [banner addSubview:self.statusLabel]; [banner addSubview:self.retryButton]; [banner addSubview:self.spinner];
    [root addSubview:banner]; [root addSubview:self.webView]; window.contentView = root;
    [NSLayoutConstraint activateConstraints:@[
        [banner.topAnchor constraintEqualToAnchor:root.topAnchor], [banner.leadingAnchor constraintEqualToAnchor:root.leadingAnchor],
        [banner.trailingAnchor constraintEqualToAnchor:root.trailingAnchor], [banner.heightAnchor constraintGreaterThanOrEqualToConstant:42],
        [self.statusLabel.leadingAnchor constraintEqualToAnchor:banner.leadingAnchor constant:16],
        [self.statusLabel.topAnchor constraintEqualToAnchor:banner.topAnchor constant:10],
        [self.statusLabel.bottomAnchor constraintEqualToAnchor:banner.bottomAnchor constant:-10],
        [self.statusLabel.trailingAnchor constraintLessThanOrEqualToAnchor:self.retryButton.leadingAnchor constant:-12],
        [self.retryButton.centerYAnchor constraintEqualToAnchor:banner.centerYAnchor],
        [self.retryButton.trailingAnchor constraintEqualToAnchor:self.spinner.leadingAnchor constant:-12],
        [self.spinner.trailingAnchor constraintEqualToAnchor:banner.trailingAnchor constant:-16],
        [self.spinner.centerYAnchor constraintEqualToAnchor:banner.centerYAnchor],
        [self.spinner.widthAnchor constraintEqualToConstant:16], [self.spinner.heightAnchor constraintEqualToConstant:16],
        [self.webView.topAnchor constraintEqualToAnchor:banner.bottomAnchor], [self.webView.bottomAnchor constraintEqualToAnchor:root.bottomAnchor],
        [self.webView.leadingAnchor constraintEqualToAnchor:root.leadingAnchor], [self.webView.trailingAnchor constraintEqualToAnchor:root.trailingAnchor]
    ]];
    qy_qq_media_reset(self.viewID,self.epoch);
    return self;
}
- (void)show {
    ((QYQQNativeWindow *)self.window).presentationAllowed=YES;
    if(self.role==1)self.host.backgroundPlayback=NO;
    [self.window makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES];
    [self.window makeFirstResponder:self.webView];
}
- (NSArray<NSToolbarItemIdentifier> *)toolbarAllowedItemIdentifiers:(NSToolbar *)toolbar { return [self toolbarDefaultItemIdentifiers:toolbar]; }
- (NSArray<NSToolbarItemIdentifier> *)toolbarDefaultItemIdentifiers:(NSToolbar *)toolbar {
    return @[@"back",@"forward",NSToolbarFlexibleSpaceItemIdentifier,@"library",@"player",@"reload"];
}
- (NSToolbarItem *)toolbar:(NSToolbar *)toolbar itemForItemIdentifier:(NSToolbarItemIdentifier)identifier willBeInsertedIntoToolbar:(BOOL)flag {
    NSDictionary *specs = @{@"back":@[@"返回",@"chevron.left",NSStringFromSelector(@selector(back:))],
        @"forward":@[@"前进",@"chevron.right",NSStringFromSelector(@selector(forward:))],
        @"library":@[@"我的音乐",@"music.note.list",NSStringFromSelector(@selector(library:))],
        @"player":@[@"播放器",@"play.rectangle",NSStringFromSelector(@selector(player:))],
        @"reload":@[@"重新载入",@"arrow.clockwise",NSStringFromSelector(@selector(reload:))]};
    NSArray *spec = specs[identifier]; if (!spec) return nil;
    NSToolbarItem *item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = spec[0]; item.paletteLabel = spec[0]; item.toolTip = spec[0];
    item.image = [NSImage imageWithSystemSymbolName:spec[1] accessibilityDescription:spec[0]];
    item.target = self; item.action = NSSelectorFromString(spec[2]); return item;
}
- (BOOL)validateToolbarItem:(NSToolbarItem *)item {
    if ([item.itemIdentifier isEqualToString:@"back"]) return self.webView.canGoBack;
    if ([item.itemIdentifier isEqualToString:@"forward"]) return self.webView.canGoForward;
    return !self.disposed;
}
- (void)back:(id)sender { [self.webView goBack]; }
- (void)forward:(id)sender { [self.webView goForward]; }
- (void)library:(id)sender { if (![self.host open:0]) [self failure:@"请先关闭不再使用的登录窗口，再打开我的音乐。" retry:NO]; }
- (void)player:(id)sender { if (![self.host open:1]) [self failure:@"请先关闭不再使用的登录窗口，再打开播放器。" retry:NO]; }
- (void)reload:(id)sender {
    if (self.retryURL && qy_navigation_url(self.retryURL)) [self.webView loadRequest:[NSURLRequest requestWithURL:self.retryURL]];
    else if (self.webView.URL && qy_navigation_url(self.webView.URL)) [self.webView reload];
    else [self.webView loadRequest:[NSURLRequest requestWithURL:[NSURL URLWithString:self.role == 1 ? QYQQPlayerURL : QYQQLibraryURL]]];
}
- (void)failure:(NSString *)message retry:(BOOL)retry {
    self.statusLabel.stringValue = message; self.statusLabel.textColor = NSColor.labelColor;
    self.retryButton.hidden = !retry; [self.spinner stopAnimation:nil];
}
- (void)invalidateDocument {
    BOOL userPaused=self.audioPausedByUser;
    if(self==self.host.audioOwner)[self.host pauseAudio:self];
    if(self==self.host.audioCandidate)self.host.audioCandidate=nil;
    self.audioPausedByUser=userPaused;
    self.pendingAudioDocument=nil;self.pendingAudioRequest=0;self.pendingAudioUser=NO;
    self.epoch++; self.ready = NO; self.hasMedia = NO; self.playing = NO; self.pollBusy = NO; self.commandBusy = NO;self.documentToken=nil;self.trackKey=nil;self.trackURL=nil;self.projectionSequence=0;self.audioProbeBusy=NO;
    qy_qq_media_reset(self.viewID,self.epoch);
}
- (void)webView:(WKWebView *)webView didStartProvisionalNavigation:(WKNavigation *)navigation {
    [self invalidateDocument]; self.retryURL = nil;
    self.statusLabel.stringValue = @"正在载入 QQ 音乐…"; self.statusLabel.textColor = NSColor.secondaryLabelColor;
    self.retryButton.hidden = YES; [self.spinner startAnimation:nil];
}
- (void)webView:(WKWebView *)webView didFinishNavigation:(WKNavigation *)navigation {
    self.ready = YES; self.retryURL = nil; self.retryButton.hidden = YES; [self.spinner stopAnimation:nil];
    if(self==self.host.libraryWorker)self.host.workerReadyAt=NSProcessInfo.processInfo.systemUptime;
    if (qy_media_url(webView.URL)) {
        if ([webView.URL.path isEqualToString:@"/n/ryqq/player"] || [webView.URL.path isEqualToString:@"/n/ryqq_v2/player"]) {
            self.role = 1; self.window.title = @"QQ 音乐播放器 · 轻屿";
        }
        self.statusLabel.stringValue = self.host.temporaryProfile ? @"在 QQ 音乐官方网站登录。此 macOS 版本的开发数据隔离模式仅保留本次登录。" :
            @"在 QQ 音乐官方网站登录，查看喜欢和歌单。关闭窗口后音乐可继续播放。";
    } else self.statusLabel.stringValue = @"QQ / 微信官方登录页面。请在网页内完成授权。";
    self.statusLabel.textColor = NSColor.secondaryLabelColor;
    [self.window.toolbar validateVisibleItems]; [self poll];
    [self.host advanceResume];
}
- (void)navigationFailed:(NSError *)error {
    if (error.code == NSURLErrorCancelled || self.disposed) return;
    [self invalidateDocument];
    NSURL *url = error.userInfo[NSURLErrorFailingURLErrorKey];
    self.retryURL = qy_navigation_url(url) ? url : self.webView.URL;
    [self failure:@"无法载入 QQ 音乐。请检查网络后重试。" retry:YES];
    if(self==self.host.libraryWorker && self.host.libraryQueue){
        self.host.libraryQueue=nil;qy_qq_restoration_failed("暂时无法连接 QQ 音乐，账号记录已保留，请检查网络后重试");
        [self.host releaseLibraryWorker];
    }
}
- (void)webView:(WKWebView *)webView didFailNavigation:(WKNavigation *)navigation withError:(NSError *)error { [self navigationFailed:error]; }
- (void)webView:(WKWebView *)webView didFailProvisionalNavigation:(WKNavigation *)navigation withError:(NSError *)error { [self navigationFailed:error]; }
- (void)webViewWebContentProcessDidTerminate:(WKWebView *)webView {
    [self invalidateDocument]; [self failure:@"QQ 音乐网页已停止响应，请重新载入。" retry:YES];
}
- (void)webView:(WKWebView *)webView decidePolicyForNavigationAction:(WKNavigationAction *)action decisionHandler:(void (^)(WKNavigationActionPolicy))decision {
    NSURL *url = action.request.URL;
    BOOL permitted = qy_navigation_url(url) || (qy_blank_url(url) && (!self.ready || qy_frame_is_vendor(action.sourceFrame)));
    if (permitted && !action.shouldPerformDownload) { decision(WKNavigationActionPolicyAllow); return; }
    decision(WKNavigationActionPolicyCancel);
    if (action.targetFrame && !action.targetFrame.mainFrame) return;
    NSString *scheme = url.scheme.lowercaseString ?: @"";
    if ([@[@"qqmusic",@"tencent",@"mqq",@"weixin",@"wxlogin"] containsObject:scheme]) {
        // Embedded login/playback never silently launches an external app.
        // A QR/login alternative is available in the official page itself.
        [self failure:@"此操作需要 QQ、微信或 QQ 音乐客户端。你可以留在网页，选择扫码登录或网页播放。" retry:NO];
    } else [self failure:@"这个链接无法在 QQ 音乐窗口中打开。请返回我的音乐继续。" retry:NO];
}
- (void)webView:(WKWebView *)webView decidePolicyForNavigationResponse:(WKNavigationResponse *)response decisionHandler:(void (^)(WKNavigationResponsePolicy))decision {
    if (response.canShowMIMEType) decision(WKNavigationResponsePolicyAllow);
    else { decision(WKNavigationResponsePolicyCancel); [self failure:@"此内容需要客户端打开。请选择 QQ 音乐网页中的播放功能。" retry:NO]; }
}
- (WKWebView *)webView:(WKWebView *)webView createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration forNavigationAction:(WKNavigationAction *)action windowFeatures:(WKWindowFeatures *)features {
    NSURL *url = action.request.URL;
    if (!qy_frame_is_vendor(action.sourceFrame) || (!qy_navigation_url(url) && !qy_blank_url(url))) return nil;
    NSInteger role = qy_media_url(url) && ([url.path isEqualToString:@"/n/ryqq/player"] || [url.path isEqualToString:@"/n/ryqq_v2/player"]) ? 1 : 2;
    BOOL background=self.role==3 || self.host.backgroundPlayback;
    if(background && qy_blank_url(url) && self.audioSelection==self.host.audioSelection && self.audioSelection)role=1;
    QYQQWindow *popup = [self.host create:role configuration:configuration];
    if (!popup) { [self failure:@"QQ 音乐窗口已达到上限，请先关闭不再使用的登录窗口。" retry:NO]; return nil; }
    // WebKit must receive a new view with its own supplied configuration for
    // window.open('about:blank') and opener/queue delivery to work. Audio is
    // gated until this exact document asks to take ownership.
    if(self.audioSelection && self.audioSelection==self.host.audioSelection)popup.audioSelection=self.audioSelection;
    else if(self.window.visible && qy_media_url(webView.URL) && (role==1 || qy_blank_url(url)))popup.audioSelection=[self.host beginAudioSelection];
    if(!background)[popup show];return popup.webView;
}
- (void)webViewDidClose:(WKWebView *)webView {
    // Script-initiated close follows the same retention rule as the native
    // close button, including the interval before a player's first sample.
    [self windowShouldClose:self.window];
}
- (BOOL)windowShouldClose:(NSWindow *)sender {
    if ((self.role == 0 || self.role == 2 || self.role == 3) && !self.hasMedia) { [self.host discard:self]; return NO; }
    ((QYQQNativeWindow *)sender).presentationAllowed=NO;
    [sender orderOut:nil]; return NO;
}
- (void)webView:(WKWebView *)webView runJavaScriptAlertPanelWithMessage:(NSString *)message initiatedByFrame:(WKFrameInfo *)frame completionHandler:(void (^)(void))completion {
    if (self.alertVisible || !qy_frame_is_vendor(frame)) { completion(); return; }
    self.alertVisible = YES;
    NSAlert *alert = [NSAlert new]; alert.messageText = @"QQ 音乐";
    alert.informativeText = [message substringToIndex:MIN(message.length,2048)]; [alert addButtonWithTitle:@"好"];
    [alert beginSheetModalForWindow:self.window completionHandler:^(NSModalResponse response){ (void)response; self.alertVisible = NO; completion(); }];
}
- (void)webView:(WKWebView *)webView runJavaScriptConfirmPanelWithMessage:(NSString *)message initiatedByFrame:(WKFrameInfo *)frame completionHandler:(void (^)(BOOL))completion {
    if (self.alertVisible || !qy_frame_is_vendor(frame)) { completion(NO); return; }
    self.alertVisible = YES;
    NSAlert *alert = [NSAlert new]; alert.messageText = @"QQ 音乐";
    alert.informativeText = [message substringToIndex:MIN(message.length,2048)];
    [alert addButtonWithTitle:@"继续"]; [alert addButtonWithTitle:@"取消"];
    [alert beginSheetModalForWindow:self.window completionHandler:^(NSModalResponse response){ self.alertVisible = NO; completion(response == NSAlertFirstButtonReturn); }];
}
- (void)acceptSnapshot:(id)value {
    if (self.disposed || !self.ready || !qy_media_url(self.webView.URL)) return;
    if (value == NSNull.null || !value) {
        // No unscoped null may erase a newer document/track. The stamped
        // projection publishes intentional empty state; Rust expires silence.
        return;
    }
    if([value isKindOfClass:NSDictionary.class] && value[@"document"]){
        NSDictionary *projection=value;
        if(projection.count!=6 || ![projection[@"origin"] isEqual:@"https://y.qq.com"] ||
            ![projection[@"document"] isKindOfClass:NSString.class] || [projection[@"document"] length]>128)return;
        if(![projection[@"sequence"] isKindOfClass:NSNumber.class])return;
        double numericSequence=[projection[@"sequence"] doubleValue];
        if(!isfinite(numericSequence) || numericSequence<1 || numericSequence>9007199254740991.0 || floor(numericSequence)!=numericSequence)return;
        uint64_t sequence=[projection[@"sequence"] unsignedLongLongValue];if(!sequence || sequence<=self.projectionSequence)return;
        NSString *token=projection[@"document"];
        if(self.documentToken && ![self.documentToken isEqualToString:token])return;
        self.documentToken=token;
        NSDictionary *media=[projection[@"media"] isKindOfClass:NSDictionary.class]?projection[@"media"]:nil;
        NSUInteger budget=0;
        if(!qy_projection_bounded(projection,0,&budget))return;
        self.projectionSequence=sequence;
        self.lastProjectionAt=NSProcessInfo.processInfo.systemUptime;
        self.hasMedia=media!=nil;self.playing=[media[@"playing"] boolValue];
        self.trackKey=[media[@"track_key"] isKindOfClass:NSString.class]?media[@"track_key"]:nil;
        self.trackURL=[media[@"track_url"] isKindOfClass:NSString.class]?media[@"track_url"]:nil;
        if(self.playing && (self!=self.host.audioOwner || self.audioSuspended)){
            self.playing=NO;
            NSMutableDictionary *safeMedia=[media mutableCopy];safeMedia[@"playing"]=@NO;
            NSMutableDictionary *safeProjection=[projection mutableCopy];safeProjection[@"media"]=safeMedia;projection=safeProjection;
        }
        NSData *json=[NSJSONSerialization dataWithJSONObject:projection options:0 error:nil];
        if(json.length && json.length<=96*1024)qy_qq_projection_update(self.viewID,self.epoch,json.bytes,json.length);
        [self.host requestAudio:self];[self.host audioDidPlay:self];[self probeAudio];
        return;
    }
    // The ten validated scalar fields are the complete schema. Reject unknown
    // keys before JSON serialization so nested/unbounded extras cannot enter it.
    if (![value isKindOfClass:NSDictionary.class] || [value count] != 10) return;
    NSDictionary *snapshot = value;
    if (![snapshot[@"origin"] isEqual:@"https://y.qq.com"]) return;
    for (NSString *key in @[@"origin",@"title",@"artist",@"album",@"artwork"]) {
        id text = snapshot[key]; if (![text isKindOfClass:NSString.class] || [text length] > 4096) return;
    }
    for (NSString *key in @[@"playing",@"position",@"duration",@"volume",@"muted"]) {
        id number = snapshot[key]; if (![number isKindOfClass:NSNumber.class] || !isfinite([number doubleValue])) return;
    }
    // A media element with unavailable text still keeps its player alive on
    // window close. Rust deliberately refuses to invent track metadata.
    self.hasMedia = YES; self.playing = [snapshot[@"playing"] boolValue];
    NSData *json = [NSJSONSerialization dataWithJSONObject:snapshot options:0 error:nil];
    if (json.length && json.length <= 16 * 1024) qy_qq_media_update(self.viewID,self.epoch,json.bytes,json.length);
}
- (void)userContentController:(WKUserContentController *)content didReceiveScriptMessage:(WKScriptMessage *)message {
    WKSecurityOrigin *origin = message.frameInfo.securityOrigin;
    if (message.webView != self.webView || !message.frameInfo.mainFrame ||
        ![origin.protocol isEqualToString:@"https"] || ![origin.host isEqualToString:@"y.qq.com"] ||
        (origin.port && origin.port != 443)) return;
    if([message.body isKindOfClass:NSDictionary.class] && [message.body[@"action"] isEqual:@"claimAudio"]){
        NSDictionary *claim=message.body;NSString *document=claim[@"document"];NSNumber *request=claim[@"request"];
        if(claim.count!=4 || ![document isKindOfClass:NSString.class] || !document.length || document.length>128 ||
            ![request isKindOfClass:NSNumber.class] || request.doubleValue<1 || request.doubleValue>9007199254740991.0 || floor(request.doubleValue)!=request.doubleValue ||
            ![claim[@"user"] isKindOfClass:NSNumber.class])return;
        if(self.documentToken && ![self.documentToken isEqualToString:document])return;
        if([self.pendingAudioDocument isEqual:document] && request.unsignedLongLongValue<=self.pendingAudioRequest)return;
        self.pendingAudioDocument=document;self.pendingAudioRequest=request.unsignedLongLongValue;self.pendingAudioUser=[claim[@"user"] boolValue];
        [self.host requestAudio:self];return;
    }
    double now = NSProcessInfo.processInfo.systemUptime;
    if (now - self.lastMessageAt < 0.2) return;
    self.lastMessageAt = now;
    // The first trusted document token comes from didFinish's epoch-bound poll,
    // never an unsolicited message from a document being replaced.
    if(self.documentToken)[self acceptSnapshot:message.body];
    [self diagnostics];
}
- (void)poll {
    if (self.disposed || !self.ready || !qy_media_url(self.webView.URL)) return;
    double now = NSProcessInfo.processInfo.systemUptime;
    if (self.pollBusy) {
        // A delayed read does not replace the document. Its event heartbeat
        // can still deliver fresh samples, and Rust expires stale samples on
        // its own. Keep this one in-flight read bounded until its completion
        // or a real navigation/process termination invalidates the epoch.
        return;
    }
    self.pollBusy = YES; self.pollStarted = now;
    uint64_t epoch = self.epoch; __weak QYQQWindow *weak = self;
    [self.webView evaluateJavaScript:@"window.__qingyuQQMusic ? window.__qingyuQQMusic.projection() : null" completionHandler:^(id result,NSError *error){
        QYQQWindow *window = weak;
        if (!window || window.disposed || window.epoch != epoch) return;
        window.pollBusy = NO;
        if (!error) [window acceptSnapshot:result];
        [window diagnostics];
    }];
}
- (void)probeAudio {
    // A suspended native media state proves an actual AudioContext/embedded
    // playback attempt even when the top document exposes only public DOM.
    if(self.audioProbeBusy || self.disposed || !self.ready || !self.hasMedia || self.audioPausedByUser ||
        self.pendingAudioRequest || self==self.host.audioOwner || !self.audioGateReady ||
        !self.audioSelection || self.audioSelection!=self.host.audioSelection)return;
    self.audioProbeBusy=YES;uint64_t epoch=self.epoch,selection=self.audioSelection;NSString *document=self.documentToken;
    __weak QYQQWindow *weak=self;
    [self.webView requestMediaPlaybackStateWithCompletionHandler:^(WKMediaPlaybackState state){
        QYQQWindow *window=weak;if(!window || window.disposed || window.epoch!=epoch)return;
        window.audioProbeBusy=NO;
        if(state!=WKMediaPlaybackStateSuspended || window.audioPausedByUser || window.pendingAudioRequest ||
            window.audioSelection!=selection || selection!=window.host.audioSelection || ![window.documentToken isEqual:document])return;
        [window.webView callAsyncJavaScript:@"const b=window.__qingyuQQMusic;return b?.projection()?.document===document && b.requestNativeAudio();"
            arguments:@{@"document":document} inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:nil];
    }];
}
- (void)diagnostics {
    NSString *directory = NSProcessInfo.processInfo.environment[@"QINGYU_QQ_DIAGNOSTICS_DIR"];
    double now = NSProcessInfo.processInfo.systemUptime;
    if (!directory.length || self.diagnosticBusy || now-self.lastDiagnosticAt < 3 || self.disposed || !self.ready || !qy_media_url(self.webView.URL)) return;
    self.diagnosticBusy = YES; self.lastDiagnosticAt = now;
    uint64_t epoch=self.epoch; __weak QYQQWindow *weak=self;
    [self.webView evaluateJavaScript:@"window.__qingyuQQMusic ? window.__qingyuQQMusic.diagnostics() : null" completionHandler:^(id result,NSError *error){
        QYQQWindow *window=weak;
        if (!window || window.disposed || window.epoch!=epoch) return;
        window.diagnosticBusy=NO;
        if (error || !qy_media_url(window.webView.URL) || ![result isKindOfClass:NSDictionary.class]) return;
        NSMutableDictionary *diagnostic=[result mutableCopy];
        diagnostic[@"nativeAudio"]=@{@"views":@(window.host.windows.count),@"owner":@(window==window.host.audioOwner),
            @"candidate":@(window==window.host.audioCandidate),@"suspended":@(window.audioSuspended),
            @"gateReady":@(window.audioGateReady),@"pausedByUser":@(window.audioPausedByUser),@"retired":@(window.audioRetired)};
        double now=NSProcessInfo.processInfo.systemUptime;
        diagnostic[@"nativeSync"]=@{@"pollBusy":@(window.pollBusy),@"pollAge":@(window.pollBusy?MAX(0,now-window.pollStarted):0),
            @"sampleAge":@(window.lastProjectionAt>0?MAX(0,now-window.lastProjectionAt):-1),@"sequence":@(window.projectionSequence)};
        NSData *json=[NSJSONSerialization dataWithJSONObject:diagnostic options:NSJSONWritingPrettyPrinted error:nil];
        if (!json.length || json.length>96*1024) return;
        [NSFileManager.defaultManager createDirectoryAtPath:directory withIntermediateDirectories:YES attributes:nil error:nil];
        NSString *name=window.role==1?@"qq-dom-player.json":window.role==0?@"qq-dom-library.json":@"qq-dom-auxiliary.json";
        [json writeToFile:[directory stringByAppendingPathComponent:name] options:NSDataWritingAtomic error:nil];
    }];
}
- (void)tearDown {
    if (self.disposed) return;
    self.disposed = YES; self.ready = NO;
    [self revokeAudio:++self.host.audioLease];
    qy_qq_media_remove(self.viewID);
    [self.webView.configuration.userContentController removeScriptMessageHandlerForName:QYQQHandler];
    [self.webView.configuration.userContentController removeAllUserScripts];
    self.webView.navigationDelegate = nil; self.webView.UIDelegate = nil;
    WKWebView *view=self.webView;
    [view pauseAllMediaPlaybackWithCompletionHandler:^{[view setAllMediaPlaybackSuspended:YES completionHandler:nil];}];
    [view stopLoading]; [view removeFromSuperview]; self.webView = nil;
    self.window.delegate = nil; [self.window orderOut:nil]; [self.window close];
}
- (void)revokeAudio:(uint64_t)lease {
    NSString *document=self.documentToken?:self.pendingAudioDocument;
    if(document.length)[self.webView callAsyncJavaScript:@"const b=window.__qingyuQQMusic;if(b?.projection()?.document===document)b.setAudioLease(lease,false);"
        arguments:@{@"lease":@(lease),@"document":document} inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:nil];
    self.pendingAudioDocument=nil;self.pendingAudioRequest=0;self.pendingAudioUser=NO;
}
@end

@interface QYQQCommand : NSObject {
    _Atomic(bool) _finished;
}
@property(strong) dispatch_semaphore_t semaphore;
@property BOOL success;
- (void)finish:(BOOL)success;
@end
@implementation QYQQCommand
- (void)finish:(BOOL)success {
    if (atomic_exchange(&_finished,true)) return;
    self.success = success;
    atomic_store(&commandPending,false);
    dispatch_semaphore_signal(self.semaphore);
}
@end

int32_t qy_qq_open(int32_t page, const char *identifier, bool isolated, const char *script) {
    if (!NSThread.isMainThread) return -1;
    @autoreleasepool {
        @try {
            if (!coordinator) {
                NSString *profile = identifier ? [NSString stringWithUTF8String:identifier] : nil;
                NSString *source = script ? [NSString stringWithUTF8String:script] : nil;
                NSUUID *uuid = [[NSUUID alloc] initWithUUIDString:profile];
                if (!uuid || !source.length) return 0;
                QYQQCoordinator *created = [QYQQCoordinator new];
                created.windows = [NSMutableArray new]; created.script = source; created.profile = profile;
                if (@available(macOS 14.0,*)) created.store = [WKWebsiteDataStore dataStoreForIdentifier:uuid];
                else {
                    // Public WebKit APIs before macOS 14 cannot name a custom
                    // persistent store. An explicit dev directory must never
                    // share production cookies, so it uses a session store.
                    created.store = isolated ? WKWebsiteDataStore.nonPersistentDataStore : WKWebsiteDataStore.defaultDataStore;
                    created.temporaryProfile = isolated;
                }
                __weak QYQQCoordinator *weak = created;
                created.pulse = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER,0,0,dispatch_get_main_queue());
                dispatch_source_set_timer(created.pulse,dispatch_time(DISPATCH_TIME_NOW,NSEC_PER_SEC),NSEC_PER_SEC,NSEC_PER_SEC/10);
                dispatch_source_set_event_handler(created.pulse,^{
                    for (QYQQWindow *window in weak.windows.copy) [window poll];
                    [weak advanceLibrary];
                    [weak advanceResume];
                });
                dispatch_resume(created.pulse);
                coordinator = created;
            }
            return (page==3?YES:page==2?[coordinator refreshLibrary]:[coordinator open:page == 1 ? 1 : 0]) ? 1 : 0;
        } @catch (NSException *exception) { return 0; }
    }
}
void qy_qq_shutdown(void) {
    if (!NSThread.isMainThread) { dispatch_async(dispatch_get_main_queue(),^{ qy_qq_shutdown(); }); return; }
    [coordinator shutdown]; coordinator = nil;
}
bool qy_qq_play_playlist(const char *value) {
    if(!NSThread.isMainThread || !coordinator || !value)return false;
    NSString *url=[NSString stringWithUTF8String:value];
    return url.length<=2048 && [coordinator playPlaylist:url];
}
bool qy_qq_start(const char *value,uint64_t request){
    if(!NSThread.isMainThread || !coordinator || !value || !request)return false;
    size_t length=strnlen(value,8193);if(length>8192)return false;
    NSData *data=[NSData dataWithBytes:value length:length];NSDictionary *descriptor=[NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    if(![descriptor isKindOfClass:NSDictionary.class] || descriptor.count!=4)return false;
    return [coordinator startResume:descriptor identifier:request];
}
bool qy_qq_set_playing(uint64_t view,uint64_t epoch,const char *document,const char *track,bool playing,uint64_t identifier){
    if(!NSThread.isMainThread || !coordinator || !document || !track || !identifier)return false;
    NSString *documentToken=[NSString stringWithUTF8String:document],*trackKey=[NSString stringWithUTF8String:track];
    if(!documentToken.length || documentToken.length>128 || !trackKey.length || trackKey.length>4096)return false;
    QYQQPlaybackRequest *request=[QYQQPlaybackRequest new];
    request.requestID=identifier;request.viewID=view;request.epoch=epoch;request.document=documentToken;request.track=trackKey;request.playing=playing;
    request.expires=NSProcessInfo.processInfo.systemUptime+1.2;
    [coordinator queuePlayback:request];return true;
}
bool qy_qq_control(uint64_t view, uint64_t epoch, int32_t operation, double value) {
    @autoreleasepool {
    // Waiting on the UI thread would deadlock the completion callback. A single
    // outstanding dispatch and one script per view bound work during hangs.
    if (NSThread.isMainThread || !isfinite(value) || operation < 0 || operation > 5) return false;
    bool expected = false;
    if (!atomic_compare_exchange_strong(&commandPending,&expected,true)) return false;
    QYQQCommand *request = [QYQQCommand new]; request.semaphore = dispatch_semaphore_create(0);
    double deadline = NSProcessInfo.processInfo.systemUptime + 0.75;
    double wallDeadline = NSDate.date.timeIntervalSince1970 * 1000.0 + 750.0;
    dispatch_async(dispatch_get_main_queue(),^{
        if (NSProcessInfo.processInfo.systemUptime > deadline) { [request finish:NO]; return; }
        QYQQWindow *target = nil;
        for (QYQQWindow *window in coordinator.windows) if (window.viewID == view && window.epoch == epoch) { target = window; break; }
        if (!target || target.disposed || !target.ready || !qy_media_url(target.webView.URL) || target.commandBusy) { [request finish:NO]; return; }
        BOOL togglePause=operation==0 && target.playing;
        if(operation==0){
            if(togglePause){[coordinator finishResume:NO seek:0 message:@"已暂停 QQ 音乐"];target.audioPausedByUser=YES;}
            else {target.audioSelection=[coordinator beginAudioSelectionFor:target];target.audioPausedByUser=NO;}
        }
        target.commandBusy = YES; __weak QYQQWindow *weak = target;
        NSDictionary *arguments = @{@"operation":@(operation),@"value":@(value),@"deadline":@(wallDeadline)};
        [target.webView callAsyncJavaScript:@"if (!window.__qingyuQQMusic) return false; return await window.__qingyuQQMusic.command({operation,value,deadline});"
            arguments:arguments inFrame:nil inContentWorld:WKContentWorld.pageWorld completionHandler:^(id result,NSError *error){
                QYQQWindow *window = weak;
                BOOL valid = window && !window.disposed && window.epoch == epoch && qy_media_url(window.webView.URL);
                if (valid) { window.commandBusy = NO;if(togglePause)[window.host pauseAudio:window];[window poll]; }
                [request finish:valid && !error && [result isKindOfClass:NSNumber.class] && [result boolValue] && NSProcessInfo.processInfo.systemUptime <= deadline];
            }];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW,(int64_t)(0.8 * NSEC_PER_SEC)),dispatch_get_main_queue(),^{ [request finish:NO]; });
    });
    if (dispatch_semaphore_wait(request.semaphore,dispatch_time(DISPATCH_TIME_NOW,(int64_t)(0.9 * NSEC_PER_SEC))) != 0) return false;
    return request.success;
    }
}
