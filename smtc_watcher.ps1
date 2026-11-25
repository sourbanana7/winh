param(
    [int]$ParentPid = 0,
    [int]$PollMs = 750
)

$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

Add-Type -AssemblyName System.Runtime.WindowsRuntime

$asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and
    $_.IsGenericMethodDefinition -and
    $_.GetParameters().Count -eq 1 -and
    $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
}) | Select-Object -First 1

function Await-WinRt($asyncOp, $resultType) {
    if (-not $asyncOp) { return $null }
    try {
        $closedMethod = $asTaskGeneric.MakeGenericMethod($resultType)
        $task = $closedMethod.Invoke($null, @($asyncOp))
        $task.Wait()
        return $task.Result
    } catch {
        return $null
    }
}

function Detect-MimeType([byte[]]$bytes, [string]$reportedType) {
    if ($reportedType -and $reportedType -match '^image/[a-z0-9.+-]+$') {
        return $reportedType.ToLowerInvariant()
    }
    if ($bytes -and $bytes.Length -ge 4) {
        if ($bytes[0] -eq 0x89 -and $bytes[1] -eq 0x50 -and $bytes[2] -eq 0x4E -and $bytes[3] -eq 0x47) {
            return 'image/png'
        }
        if ($bytes[0] -eq 0xFF -and $bytes[1] -eq 0xD8) {
            return 'image/jpeg'
        }
        if ($bytes[0] -eq 0x47 -and $bytes[1] -eq 0x49 -and $bytes[2] -eq 0x46) {
            return 'image/gif'
        }
        if ($bytes[0] -eq 0x42 -and $bytes[1] -eq 0x4D) {
            return 'image/bmp'
        }
        if ($bytes.Length -ge 12 -and $bytes[0] -eq 0x52 -and $bytes[1] -eq 0x49 -and $bytes[2] -eq 0x46 -and $bytes[3] -eq 0x46 -and
            $bytes[8] -eq 0x57 -and $bytes[9] -eq 0x45 -and $bytes[10] -eq 0x42 -and $bytes[11] -eq 0x50) {
            return 'image/webp'
        }
    }
    return 'image/jpeg'
}

function Get-PreferredSessionInfo($mgr, $lastSession = $null, [int64]$lastSeekAtMs = 0L, [int64]$nowMs = 0L) {
    $current = $null
    try { $current = $mgr.GetCurrentSession() } catch {}
    $all = $null
    try { $all = $mgr.GetSessions() } catch {}

    $entries = New-Object System.Collections.Generic.List[object]
    $seenRefs = New-Object System.Collections.Generic.List[object]

    $addCandidate = {
        param($sess, [bool]$isCurrent)
        if (-not $sess) { return }
        foreach ($existing in $seenRefs) {
            if ([object]::ReferenceEquals($sess, $existing)) { return }
        }
        [void]$seenRefs.Add($sess)
        try {
            $playbackInfo = $sess.GetPlaybackInfo()
            $status = if ($playbackInfo) { $playbackInfo.PlaybackStatus.ToString() } else { 'Unknown' }
            if ($status -eq 'Closed') { return }
            $baseScore = switch ($status) {
                'Playing'  { 100 }
                'Changing' { 90 }
                'Paused'   { 40 }
                'Opened'   { 20 }
                default    { 0 }
            }
            if ($isCurrent) { $baseScore += 10 }

            if ($lastSession -and [object]::ReferenceEquals($sess, $lastSession)) {
                if ($status -ne 'Closed' -and $status -ne 'Stopped') {
                    $baseScore += 5
                    if ($lastSeekAtMs -gt 0L -and $nowMs -gt 0L -and ($nowMs - $lastSeekAtMs) -lt 1500L) {
                        $baseScore += 70
                    }
                }
            }

            [void]$entries.Add([PSCustomObject]@{
                Session = $sess
                PlaybackInfo = $playbackInfo
                Status = $status
                BaseScore = $baseScore
            })
        } catch {}
    }

    if ($current) { & $addCandidate $current $true }
    if ($all) {
        foreach ($s in $all) {
            & $addCandidate $s $false
        }
    }

    if ($entries.Count -eq 0) { return $null }

    # Sort descending by synchronous PlaybackStatus score so we avoid querying background paused sessions when an active Playing session has a title
    $sorted = $entries | Sort-Object -Property BaseScore -Descending
    $best = $null
    $bestScore = -999

    foreach ($entry in $sorted) {
        # Short-circuit if we already found a Playing session with a valid title and remaining candidates cannot beat it
        if ($best -and $bestScore -ge ($entry.BaseScore + 25)) {
            break
        }
        try {
            $props = Await-WinRt ($entry.Session.TryGetMediaPropertiesAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties])
            $title = if ($props -and $props.Title) { [string]$props.Title } else { '' }
            $score = $entry.BaseScore + $(if ($title.Trim().Length -gt 0) { 25 } else { 0 })

            if ($score -gt $bestScore) {
                $appId = if ($entry.Session.SourceAppUserModelId) { [string]$entry.Session.SourceAppUserModelId } else { '' }
                $bestScore = $score
                $best = [PSCustomObject]@{
                    Session = $entry.Session
                    Props = $props
                    PlaybackInfo = $entry.PlaybackInfo
                    Status = $entry.Status
                    AppId = $appId
                    Title = $title
                }
            }
        } catch {
            continue
        }
    }

    return $best
}

function Invoke-SessionCommand($session, [string]$cmd) {
    if (-not $session) { return $false }
    try {
        if ($cmd.StartsWith('seek:')) {
            $tickStr = $cmd.Substring(5)
            $offsetTicks = 0L
            if ([int64]::TryParse($tickStr, [ref]$offsetTicks)) {
                $tl = $null
                try { $tl = $session.GetTimelineProperties() } catch {}
                $startTicks = if ($tl) { $tl.StartTime.Ticks } else { 0L }
                $targetTicks = $startTicks + $offsetTicks
                if ($tl -and $tl.MaxSeekTime.Ticks -gt $tl.MinSeekTime.Ticks) {
                    if ($targetTicks -lt $tl.MinSeekTime.Ticks) { $targetTicks = $tl.MinSeekTime.Ticks }
                    if ($targetTicks -gt $tl.MaxSeekTime.Ticks) { $targetTicks = $tl.MaxSeekTime.Ticks }
                } elseif ($tl -and $tl.EndTime.Ticks -gt $startTicks) {
                    $span = $tl.EndTime.Ticks - $startTicks
                    $safety = 2000000L # 200ms
                    $maxTicks = if ($span -gt 20000000L) { $tl.EndTime.Ticks - $safety } else { $tl.EndTime.Ticks }
                    if ($targetTicks -lt $startTicks) { $targetTicks = $startTicks }
                    if ($targetTicks -gt $maxTicks) { $targetTicks = $maxTicks }
                } elseif ($targetTicks -lt 0L) {
                    $targetTicks = 0L
                }
                return [bool](Await-WinRt ($session.TryChangePlaybackPositionAsync($targetTicks)) ([bool]))
            }
            return $false
        }
        switch ($cmd) {
            'play-pause' { return [bool](Await-WinRt ($session.TryTogglePlayPauseAsync()) ([bool])) }
            'play'       { return [bool](Await-WinRt ($session.TryPlayAsync()) ([bool])) }
            'pause'      { return [bool](Await-WinRt ($session.TryPauseAsync()) ([bool])) }
            'next'       { return [bool](Await-WinRt ($session.TrySkipNextAsync()) ([bool])) }
            'previous'   { return [bool](Await-WinRt ($session.TrySkipPreviousAsync()) ([bool])) }
            default      { return $false }
        }
    } catch {
        return $false
    }
}

[Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager, Windows.Media, ContentType = WindowsRuntime] | Out-Null
[Windows.Storage.Streams.DataReader, Windows.Storage.Streams, ContentType = WindowsRuntime] | Out-Null

$mgr = $null
for ($attempt = 0; $attempt -lt 5; $attempt++) {
    $mgr = Await-WinRt ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])
    if ($mgr) { break }
    Start-Sleep -Milliseconds 600
}

if (-not $mgr) {
    Write-Output 'SMTC_ERROR:Failed to initialize GlobalSystemMediaTransportControlsSessionManager'
    exit 1
}

$lastMetaSig = ''
$lastEmittedPos = 0.0
$lastEmittedAtMs = 0L
$lastEmittedStatus = 'Stopped'
$lastRawPos = -1.0

$lastThumbTrackKey = ''
$lastThumbBase64 = ''
$lastThumbMime = ''
$lastSession = $null

$lastSeekAtMs = 0L
$lastSeekSession = $null
$lastSeekTargetSec = 0.0
$lastSeekWasPlaying = $false

$useStdin = [Console]::IsInputRedirected
$cmdQueue = $null
$stdinPs = $null
if ($useStdin) {
    try {
        $cmdQueue = New-Object 'System.Collections.Concurrent.ConcurrentQueue[string]'
        $stdinPs = [powershell]::Create()
        [void]$stdinPs.AddScript({
            param($queue)
            try {
                $stream = [Console]::OpenStandardInput()
                $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)
                while ($true) {
                    $line = $reader.ReadLine()
                    if ($null -eq $line) {
                        $queue.Enqueue('__EOF__')
                        break
                    }
                    $queue.Enqueue($line)
                }
            } catch {
                $queue.Enqueue('__EOF__')
            }
        }).AddArgument($cmdQueue)
        [void]$stdinPs.BeginInvoke()
    } catch {
        $useStdin = $false
    }
}

$loopCount = 0

while ($true) {
    $loopCount++

    # Check parent process liveness periodically if ParentPid provided
    if ($ParentPid -gt 0 -and ($loopCount % 6) -eq 0) {
        $parentProc = Get-Process -Id $ParentPid -ErrorAction SilentlyContinue
        if (-not $parentProc) { [Environment]::Exit(0) }
    }

    # Handle incoming transport commands from background stdin queue without blocking
    $hadCommand = $false
    if ($useStdin -and $cmdQueue) {
        $line = $null
        while ($cmdQueue.TryDequeue([ref]$line)) {
            if ($line -eq '__EOF__') {
                if ($ParentPid -gt 0) {
                    [Environment]::Exit(0)
                }
                $useStdin = $false
                break
            }
            $trimmed = $line.Trim()
            if ($trimmed -eq 'CMD:quit') {
                [Environment]::Exit(0)
            }
            if ($trimmed.StartsWith('CMD:')) {
                $cmdName = $trimmed.Substring(4).ToLowerInvariant()
                $targetSession = $lastSession
                if (-not $targetSession) {
                    try { $targetSession = $mgr.GetCurrentSession() } catch {}
                }

                if ($cmdName.StartsWith('seek:')) {
                    $pInfo = $null
                    try { $pInfo = $targetSession.GetPlaybackInfo() } catch {}
                    $statusBeforeSeek = if ($pInfo) { $pInfo.PlaybackStatus.ToString() } else { '' }
                    $wasPlaying = ($statusBeforeSeek -eq 'Playing' -or $lastEmittedStatus -eq 'Playing')

                    $seekTickStr = $cmdName.Substring(5)
                    $seekTicksVal = 0L
                    $targetSecVal = if ([int64]::TryParse($seekTickStr, [ref]$seekTicksVal)) { [Math]::Max(0.0, $seekTicksVal / 10000000.0) } else { 0.0 }

                    if (Invoke-SessionCommand $targetSession $cmdName) {
                        $nowSeekMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
                        $lastSeekAtMs = $nowSeekMs
                        $lastSeekSession = $targetSession
                        $lastSeekTargetSec = $targetSecVal
                        $lastSeekWasPlaying = $wasPlaying
                    }
                } else {
                    if (Invoke-SessionCommand $targetSession $cmdName) {
                        $lastMetaSig = ''
                        $lastSeekAtMs = 0L
                        $lastSeekSession = $null
                    }
                }
                $hadCommand = $true
            }
        }
        if ($hadCommand) {
            Start-Sleep -Milliseconds 80
        }
    }

    try {
        $nowMs = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        $info = Get-PreferredSessionInfo $mgr $lastSession $lastSeekAtMs $nowMs

        if ($info -and $info.Session) {
            if ($lastSeekSession -and -not [object]::ReferenceEquals($info.Session, $lastSeekSession)) {
                $lastSeekAtMs = 0L
                $lastSeekSession = $null
            }
        }

        if ($info -and ($info.Title.Trim().Length -gt 0 -or $info.Status -eq 'Playing')) {
            $session = $info.Session
            $lastSession = $session
            $props = $info.Props
            $appId = $info.AppId
            $title = $info.Title
            $artist = if ($props -and $props.Artist) { [string]$props.Artist } elseif ($props -and $props.AlbumArtist) { [string]$props.AlbumArtist } else { '' }
            $album = if ($props -and $props.AlbumTitle) { [string]$props.AlbumTitle } else { '' }
            $playbackStatus = $info.Status

            $timeline = $null
            try { $timeline = $session.GetTimelineProperties() } catch {}
            $startSec = if ($timeline) { $timeline.StartTime.TotalSeconds } else { 0.0 }
            $endSec = if ($timeline) { $timeline.EndTime.TotalSeconds } else { 0.0 }
            $duration = [Math]::Max(0.0, $endSec - $startSec)
            $rawPos = if ($timeline) { [Math]::Max(0.0, $timeline.Position.TotalSeconds - $startSec) } else { 0.0 }

            $inSeekGrace = ($lastSeekSession -and [object]::ReferenceEquals($session, $lastSeekSession) -and ($nowMs - $lastSeekAtMs) -lt 900L)
            if ($inSeekGrace) {
                $timeSinceSeekMs = $nowMs - $lastSeekAtMs
                if ($lastSeekWasPlaying) {
                    if ($playbackStatus -eq 'Changing' -or ($playbackStatus -eq 'Paused' -and $timeSinceSeekMs -lt 450L)) {
                        $playbackStatus = 'Playing'
                    }
                }
                if ([Math]::Abs($rawPos - $lastSeekTargetSec) -le 1.5) {
                    $lastSeekAtMs = 0L
                    $lastSeekSession = $null
                }
            }

            $tlUpdatedMs = 0L
            if ($timeline -and $timeline.LastUpdatedTime -and $timeline.LastUpdatedTime.Year -gt 2000) {
                $tlUpdatedMs = $timeline.LastUpdatedTime.ToUnixTimeMilliseconds()
            }

            $effectivePos = $rawPos
            if ($playbackStatus -eq 'Playing' -and $tlUpdatedMs -gt 0 -and (-not $inSeekGrace -or $tlUpdatedMs -ge $lastSeekAtMs)) {
                $elapsedSec = [Math]::Max(0.0, ($nowMs - $tlUpdatedMs) / 1000.0)
                $candidatePos = $rawPos + $elapsedSec
                if ($duration -gt 0.0) {
                    if ($candidatePos -le ($duration + 3.0)) {
                        $effectivePos = [Math]::Min($candidatePos, $duration)
                    } else {
                        $effectivePos = $rawPos
                    }
                } elseif ($elapsedSec -lt 3600.0) {
                    $effectivePos = $candidatePos
                }
            }

            # Cache thumbnail per track identity so we never re-read or re-encode the WinRT stream on steady polls
            $thumbTrackKey = "$appId|$title|$artist|$album"
            $thumbBase64 = ''
            $thumbMime = ''
            if ($props -and $props.Thumbnail) {
                if ($thumbTrackKey -eq $lastThumbTrackKey -and $lastThumbBase64.Length -gt 0) {
                    $thumbBase64 = $lastThumbBase64
                    $thumbMime = $lastThumbMime
                } else {
                    $stream = $null
                    $netStream = $null
                    $memStream = $null
                    try {
                        $stream = Await-WinRt ($props.Thumbnail.OpenReadAsync()) ([Windows.Storage.Streams.IRandomAccessStreamWithContentType])
                        if ($stream -and $stream.Size -gt 0 -and $stream.Size -lt 8388608) {
                            try { $stream.Seek([uint64]0) } catch {}
                            $bytes = $null
                            try {
                                $netStream = [System.IO.WindowsRuntimeStreamExtensions]::AsStreamForRead($stream)
                                if ($netStream -and $netStream.CanSeek) {
                                    $netStream.Position = 0
                                }
                                $memStream = New-Object System.IO.MemoryStream
                                $netStream.CopyTo($memStream)
                                $bytes = $memStream.ToArray()
                            } catch {
                                $bytes = $null
                            }

                            # Fallback via WinRT DataReader if AsStreamForRead yielded empty bytes despite non-zero Size
                            if ((-not $bytes -or $bytes.Length -eq 0) -and $stream.Size -gt 0) {
                                try {
                                    $stream.Seek([uint64]0)
                                    $reader = New-Object Windows.Storage.Streams.DataReader($stream)
                                    $loaded = Await-WinRt ($reader.LoadAsync([uint32]$stream.Size)) ([uint32])
                                    if ($loaded -gt 0) {
                                        $bytes = New-Object byte[] $loaded
                                        $reader.ReadBytes($bytes)
                                    }
                                    try { $reader.DetachStream() } catch {}
                                    if ($reader -is [System.IDisposable]) { try { $reader.Dispose() } catch {} }
                                } catch {}
                            }

                            if ($bytes -and $bytes.Length -gt 0) {
                                $thumbBase64 = [Convert]::ToBase64String($bytes)
                                $thumbMime = Detect-MimeType $bytes $stream.ContentType
                                $lastThumbTrackKey = $thumbTrackKey
                                $lastThumbBase64 = $thumbBase64
                                $lastThumbMime = $thumbMime
                            }
                        }
                    } catch {
                    } finally {
                        if ($memStream) { try { $memStream.Dispose() } catch {} }
                        if ($netStream) { try { $netStream.Dispose() } catch {} }
                        if ($stream -and ($stream -is [System.IDisposable])) { try { $stream.Dispose() } catch {} }
                    }
                }
            } else {
                if ($thumbTrackKey -ne $lastThumbTrackKey) {
                    $lastThumbTrackKey = $thumbTrackKey
                    $lastThumbBase64 = ''
                    $lastThumbMime = ''
                }
            }

            $roundedPos = [Math]::Round($effectivePos, 3)
            $roundedDur = [Math]::Round($duration, 3)
            $metaSig = "1|$thumbTrackKey|$playbackStatus|$([Math]::Round($roundedDur, 2))|$($thumbBase64.Length)"

            $shouldEmit = $false
            if ($metaSig -ne $lastMetaSig) {
                $shouldEmit = $true
            } else {
                $expected = if ($lastEmittedStatus -eq 'Playing') {
                    $lastEmittedPos + ([Math]::Max(0.0, ($nowMs - $lastEmittedAtMs) / 1000.0))
                } else {
                    $lastEmittedPos
                }
                if ([Math]::Abs($effectivePos - $expected) -gt 2.0) {
                    $shouldEmit = $true
                } elseif ($playbackStatus -ne 'Playing' -and [Math]::Abs($rawPos - $lastRawPos) -gt 0.5) {
                    $shouldEmit = $true
                }
            }

            if ($shouldEmit) {
                $data = [PSCustomObject]@{
                    active = $true
                    appId = $appId
                    title = $title
                    artist = $artist
                    album = $album
                    status = $playbackStatus
                    position = $roundedPos
                    duration = $roundedDur
                    lastUpdatedMs = $nowMs
                    thumbnail = $thumbBase64
                    thumbnailMime = $thumbMime
                }
                $json = $data | ConvertTo-Json -Compress
                Write-Output "MEDIA_JSON:$json"
                [Console]::Out.Flush()

                $lastMetaSig = $metaSig
                $lastEmittedPos = $effectivePos
                $lastEmittedAtMs = $nowMs
                $lastEmittedStatus = $playbackStatus
                $lastRawPos = $rawPos
            }
        } else {
            $lastSession = $null
            $metaSig = '0|Stopped'
            if ($metaSig -ne $lastMetaSig) {
                $lastThumbTrackKey = ''
                $lastThumbBase64 = ''
                $lastThumbMime = ''
                $data = [PSCustomObject]@{
                    active = $false
                    appId = ''
                    title = ''
                    artist = ''
                    album = ''
                    status = 'Stopped'
                    position = 0
                    duration = 0
                    lastUpdatedMs = $nowMs
                    thumbnail = ''
                    thumbnailMime = ''
                }
                $json = $data | ConvertTo-Json -Compress
                Write-Output "MEDIA_JSON:$json"
                [Console]::Out.Flush()

                $lastMetaSig = $metaSig
                $lastEmittedPos = 0.0
                $lastEmittedAtMs = $nowMs
                $lastEmittedStatus = 'Stopped'
                $lastRawPos = -1.0
            }
        }
    } catch {
        # ignore transient COM / WinRT session transitions
    }

    # Sleep up to $PollMs while waking immediately if a CMD arrives on stdin
    $slept = 0
    while ($slept -lt $PollMs) {
        if ($useStdin -and $cmdQueue -and -not $cmdQueue.IsEmpty) { break }
        $step = [Math]::Min(60, $PollMs - $slept)
        Start-Sleep -Milliseconds $step
        $slept += $step
    }
}
