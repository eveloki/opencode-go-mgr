[CmdletBinding()]
param(
    [string]$BaseUrl = "http://127.0.0.1:9042",
    [string]$Model = "deepseek-v4-flash",
    [string[]]$TargetAccountNames = @("112", "115"),
    [int]$RequestTimeoutSec = 120,
    [switch]$SkipProtocolMatrix,
    [switch]$SkipConcurrency,
    [switch]$OnlyConcurrency
)

$ErrorActionPreference = "Stop"
$BaseUrl = $BaseUrl.TrimEnd('/')
$script:RestoreErrors = [System.Collections.Generic.List[string]]::new()
$script:RequestResults = [System.Collections.Generic.List[object]]::new()
$script:ScenarioResults = [System.Collections.Generic.List[object]]::new()
$script:SettingsSnapshot = $null
$script:AccountsSnapshot = $null
$script:GatewayKey = $null
$script:TestStartedAt = $null
$script:FatalError = ""

function Write-Info([string]$Message) {
    Write-Host "[routing] $Message"
}

function Write-Result([string]$Status, [string]$Message) {
    $color = if ($Status -eq "PASS") { "Green" } elseif ($Status -eq "WARN") { "Yellow" } else { "Red" }
    Write-Host "[$Status] $Message" -ForegroundColor $color
}

function Invoke-JsonRequest {
    param(
        [Parameter(Mandatory)] [string]$Uri,
        [ValidateSet("GET", "POST", "PUT", "PATCH", "DELETE")] [string]$Method = "GET",
        [hashtable]$Headers = @{},
        [object]$Body,
        [int]$TimeoutSec = $RequestTimeoutSec
    )

    $requestHeaders = @{}
    foreach ($entry in $Headers.GetEnumerator()) {
        $requestHeaders[$entry.Key] = $entry.Value
    }
    $jsonBody = $null
    if ($null -ne $Body) {
        $jsonBody = $Body | ConvertTo-Json -Depth 20 -Compress
        if (-not $requestHeaders.ContainsKey("Content-Type")) {
            $requestHeaders["Content-Type"] = "application/json"
        }
    }

    try {
        $response = Invoke-WebRequest -Uri $Uri -Method $Method -Headers $requestHeaders -Body $jsonBody -TimeoutSec $TimeoutSec
        $parsed = $null
        if ($response.Content) {
            try { $parsed = $response.Content | ConvertFrom-Json } catch { $parsed = $response.Content }
        }
        return [pscustomobject]@{
            StatusCode = [int]$response.StatusCode
            Headers = $response.Headers
            Body = $parsed
            RawBody = $response.Content
        }
    } catch {
        $statusCode = 0
        $rawBody = ""
        if ($_.Exception.Response) {
            $statusCode = [int]$_.Exception.Response.StatusCode
            try {
                $reader = [System.IO.StreamReader]::new($_.Exception.Response.GetResponseStream())
                $rawBody = $reader.ReadToEnd()
                $reader.Dispose()
            } catch { }
        }
        $detail = if ($rawBody) { $rawBody } else { $_.Exception.Message }
        throw "HTTP $Method $Uri failed with status ${statusCode}: $detail"
    }
}

function Get-Settings {
    (Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/settings").Body
}

function Get-Accounts {
    @((Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/accounts").Body)
}

function Get-ForwardLogs([int]$Limit = 100) {
    $encodedModel = [Uri]::EscapeDataString($Model)
    $result = (Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/logs/forward?limit=$Limit&offset=0&model=$encodedModel&sort_by=timestamp&sort_order=desc").Body
    @($result.items)
}

function Get-RequestId($Headers) {
    foreach ($name in @("x-ocg-request-id", "X-OCG-Request-Id")) {
        $value = $Headers[$name]
        if ($value) { return [string]$value }
    }
    return ""
}

function Set-Routing([string]$Mode, [bool]$ConversationSticky) {
    $current = Get-Settings
    $payload = [ordered]@{
        gateway_port = $current.gateway_port
        gateway_key = $current.gateway_key
        upstream_base_url = $current.upstream_base_url
        client_root_url = $current.client_root_url
        auto_start = $current.auto_start
        show_dock_icon = $current.show_dock_icon
        connect_timeout_secs = $current.connect_timeout_secs
        non_stream_timeout_secs = $current.non_stream_timeout_secs
        stream_idle_timeout_secs = $current.stream_idle_timeout_secs
        routing_mode = $Mode
        conversation_sticky = $ConversationSticky
        expected_revision = $current.revision
    }
    $response = Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/settings" -Method POST -Body $payload
    Write-Info "routing=$Mode conversation_sticky=$ConversationSticky revision=$($response.Body.revision)"
}

function Set-AccountEnabled([string]$AccountId, [bool]$Enabled) {
    $account = $script:AccountsSnapshot | Where-Object { $_.id -eq $AccountId } | Select-Object -First 1
    if ($null -eq $account) { throw "account $AccountId is not in the startup snapshot" }
    $current = Get-Accounts | Where-Object { $_.id -eq $AccountId } | Select-Object -First 1
    if ($null -eq $current) { throw "account $AccountId is missing" }
    if ([bool]$current.enabled -eq $Enabled) { return }
    [void](Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/accounts/$AccountId/toggle" -Method POST)
}

function Set-AccountOrder([string[]]$AccountIds) {
    $payload = @{ account_ids = @($AccountIds) }
    [void](Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/accounts/order" -Method PUT -Body $payload)
}

function Get-AccountByName([string]$Name) {
    $account = Get-Accounts | Where-Object { $_.name -eq $Name } | Select-Object -First 1
    if ($null -eq $account) { throw "required account '$Name' was not found" }
    return $account
}

function Assert-Equal($Actual, $Expected, [string]$Message) {
    if ($Actual -ne $Expected) { throw "$Message. expected=$Expected actual=$Actual" }
}

function Assert-Sequence([object[]]$Actual, [string[]]$Expected, [string]$Message) {
    $actualText = (@($Actual) -join ",")
    $expectedText = (@($Expected) -join ",")
    if ($actualText -ne $expectedText) {
        throw "$Message. expected=[$expectedText] actual=[$actualText]"
    }
}

function Invoke-Chat {
    param(
        [Parameter(Mandatory)] [string]$Label,
        [string]$ConversationId,
        [string]$System = "routing-live-test",
        [string]$User = "reply with exactly: ok",
        [switch]$UseTools,
        [int]$MaxTokens = 8,
        [object[]]$History = @()
    )

    $headers = @{ Authorization = "Bearer $script:GatewayKey"; "Content-Type" = "application/json" }
    if ($ConversationId) { $headers["X-OCG-Conversation-Id"] = $ConversationId }
    $messages = @(
        @{ role = "system"; content = $System },
        @{ role = "user"; content = $User }
    )
    if (@($History).Count -gt 0) {
        $messages += @($History)
    }
    $payload = [ordered]@{
        model = $Model
        stream = $false
        max_tokens = $MaxTokens
        messages = $messages
    }
    if ($UseTools) {
        $payload.tools = @(@{
            type = "function"
            function = @{
                name = "routing_probe"
                description = "A deterministic test tool"
                parameters = @{ type = "object"; properties = @{} }
            }
        })
    }

    $started = [DateTime]::UtcNow
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $response = $null
    $errorText = ""
    try {
        $response = Invoke-JsonRequest -Uri "$BaseUrl/v1/chat/completions" -Method POST -Headers $headers -Body $payload
    } catch {
        $errorText = $_.Exception.Message
    }
    $stopwatch.Stop()
    $requestId = if ($response) { Get-RequestId $response.Headers } else { "" }
    $statusCode = if ($response) { $response.StatusCode } else { 0 }
    $result = [pscustomobject]@{
        Label = $Label
        StatusCode = $statusCode
        RequestId = $requestId
        ElapsedMs = $stopwatch.ElapsedMilliseconds
        StartedAt = $started
        Error = $errorText
    }
    $script:RequestResults.Add($result)
    if ($errorText) { throw "[$Label] $errorText" }
    Assert-Equal $statusCode 200 "[$Label] request status"
    if (-not $requestId) { throw "[$Label] missing x-ocg-request-id" }
    return $result
}

function Invoke-ProtocolMatrix {
    $headers = @{ Authorization = "Bearer $script:GatewayKey"; "Content-Type" = "application/json" }
    $cases = @(
        @{ Label = "protocol-chat"; Uri = "$BaseUrl/v1/chat/completions"; Body = @{ model = $Model; messages = @(@{ role = "user"; content = "reply with exactly: ok" }); max_tokens = 8; stream = $false }; Headers = $headers },
        @{ Label = "protocol-responses"; Uri = "$BaseUrl/v1/responses"; Body = @{ model = $Model; input = "reply with exactly: ok"; max_output_tokens = 8; store = $false; stream = $false }; Headers = $headers },
        @{ Label = "protocol-messages"; Uri = "$BaseUrl/v1/messages"; Body = @{ model = $Model; messages = @(@{ role = "user"; content = "reply with exactly: ok" }); max_tokens = 8; stream = $false }; Headers = @{ "x-api-key" = $script:GatewayKey; "anthropic-version" = "2023-06-01"; "Content-Type" = "application/json" } },
        @{ Label = "protocol-gemini"; Uri = "$BaseUrl/v1beta/models/$Model`:generateContent"; Body = @{ contents = @(@{ role = "user"; parts = @(@{ text = "reply with exactly: ok" }) }) }; Headers = @{ "x-goog-api-key" = $script:GatewayKey; "Content-Type" = "application/json" } }
    )
    foreach ($case in $cases) {
        $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
        $response = Invoke-JsonRequest -Uri $case.Uri -Method POST -Headers $case.Headers -Body $case.Body
        $stopwatch.Stop()
        Assert-Equal $response.StatusCode 200 "[$($case.Label)] request status"
        $requestId = Get-RequestId $response.Headers
        if (-not $requestId) { throw "[$($case.Label)] missing x-ocg-request-id" }
        $requestResult = [pscustomobject]@{ Label = $case.Label; StatusCode = $response.StatusCode; RequestId = $requestId; ElapsedMs = $stopwatch.ElapsedMilliseconds; StartedAt = [DateTime]::UtcNow; Error = "" }
        $script:RequestResults.Add($requestResult)
        $log = Resolve-LogAccount $requestResult
        Assert-Equal $log.Status "success" "[$($case.Label)] forward log status"
        Assert-Equal $log.Model $Model "[$($case.Label)] forward log model"
        Write-Info "$($case.Label) status=$($response.StatusCode) account=$($log.Account) request_id=$requestId"
    }
}

function Resolve-LogAccount($RequestResult, [int]$TimeoutSec = 30) {
    $deadline = [DateTime]::UtcNow.AddSeconds($TimeoutSec)
    do {
        $logs = Get-ForwardLogs 100 | Where-Object { $_.request_id -eq $RequestResult.RequestId }
        if (@($logs).Count -gt 0) {
            $log = @($logs)[0]
            return [pscustomobject]@{ Account = [string]$log.account_name; AccountId = [string]$log.account_id; Status = [string]$log.status; HttpStatus = [int]$log.http_status; Model = [string]$log.model }
        }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "No forward log found for request $($RequestResult.RequestId)"
}

function Invoke-AndResolve([string]$Label, [string]$ConversationId = $null, [string]$System = "routing-live-test", [string]$User = "reply with exactly: ok", [switch]$UseTools, [object[]]$History = @()) {
    $request = Invoke-Chat -Label $Label -ConversationId $ConversationId -System $System -User $User -UseTools:$UseTools -History $History
    $log = Resolve-LogAccount $request
    Assert-Equal $log.Status "success" "[$Label] forward log status"
    Assert-Equal $log.HttpStatus 200 "[$Label] forward log HTTP status"
    Assert-Equal $log.Model $Model "[$Label] forward log model"
    Write-Info "$Label account=$($log.Account) request_id=$($request.RequestId) elapsed_ms=$($request.ElapsedMs)"
    return [pscustomobject]@{ Request = $request; Log = $log }
}

function Run-Scenario([string]$Name, [scriptblock]$Body) {
    try {
        & $Body
        $script:ScenarioResults.Add([pscustomobject]@{ Name = $Name; Status = "PASS"; Error = "" })
        Write-Result "PASS" $Name
    } catch {
        $script:ScenarioResults.Add([pscustomobject]@{ Name = $Name; Status = "FAIL"; Error = $_.Exception.Message })
        Write-Result "FAIL" "${Name}: $($_.Exception.Message)"
        throw
    }
}

function Assert-NoSensitiveRoutingLogData {
    $serialized = (Get-ForwardLogs 100 | ConvertTo-Json -Depth 20 -Compress)
    foreach ($secret in @(
        "live-routing-a",
        "live-routing-b",
        "live-routing-rebind",
        "stable-system",
        "first user",
        "previous answer",
        "latest user"
    )) {
        if ($serialized.Contains($secret, [StringComparison]::Ordinal)) {
            throw "forward logs contain routing test source text"
        }
    }
}

function Restore-State {
    if ($null -eq $script:SettingsSnapshot -or $null -eq $script:AccountsSnapshot) { return }
    try {
        $currentAccounts = Get-Accounts
        foreach ($account in $script:AccountsSnapshot) {
            $current = $currentAccounts | Where-Object { $_.id -eq $account.id } | Select-Object -First 1
            if ($null -eq $current) { throw "account $($account.id) disappeared during test" }
            if ([bool]$current.enabled -ne [bool]$account.enabled) {
                [void](Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/accounts/$($account.id)/toggle" -Method POST)
            }
        }
        Set-AccountOrder @($script:AccountsSnapshot | ForEach-Object { $_.id })
        $currentSettings = Get-Settings
        $payload = [ordered]@{
            gateway_port = $script:SettingsSnapshot.gateway_port
            gateway_key = $script:SettingsSnapshot.gateway_key
            upstream_base_url = $script:SettingsSnapshot.upstream_base_url
            client_root_url = $script:SettingsSnapshot.client_root_url
            auto_start = $script:SettingsSnapshot.auto_start
            show_dock_icon = $script:SettingsSnapshot.show_dock_icon
            connect_timeout_secs = $script:SettingsSnapshot.connect_timeout_secs
            non_stream_timeout_secs = $script:SettingsSnapshot.non_stream_timeout_secs
            stream_idle_timeout_secs = $script:SettingsSnapshot.stream_idle_timeout_secs
            routing_mode = $script:SettingsSnapshot.routing_mode
            conversation_sticky = $script:SettingsSnapshot.conversation_sticky
            expected_revision = $currentSettings.revision
        }
        [void](Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/settings" -Method POST -Body $payload)
        $verifiedSettings = Get-Settings
        $verifiedAccounts = Get-Accounts
        if ($verifiedSettings.routing_mode -ne $script:SettingsSnapshot.routing_mode -or [bool]$verifiedSettings.conversation_sticky -ne [bool]$script:SettingsSnapshot.conversation_sticky) {
            throw "settings restore verification failed"
        }
        if ((@($verifiedAccounts | ForEach-Object { $_.id }) -join ",") -ne (@($script:AccountsSnapshot | ForEach-Object { $_.id }) -join ",")) {
            throw "account order restore verification failed"
        }
        foreach ($account in $script:AccountsSnapshot) {
            $verified = $verifiedAccounts | Where-Object { $_.id -eq $account.id } | Select-Object -First 1
            if ([bool]$verified.enabled -ne [bool]$account.enabled) { throw "enabled restore verification failed for $($account.name)" }
        }
        Write-Result "PASS" "state restored to startup snapshot"
    } catch {
        $script:RestoreErrors.Add($_.Exception.Message)
        Write-Result "FAIL" "state restoration failed: $($_.Exception.Message)"
    }
}

try {
    $script:TestStartedAt = [DateTime]::UtcNow
    $script:SettingsSnapshot = Get-Settings
    $script:AccountsSnapshot = Get-Accounts
    $script:GatewayKey = [string]$script:SettingsSnapshot.gateway_key
    if (-not $script:GatewayKey) { throw "gateway key is empty" }
    if (-not $script:SettingsSnapshot.routing_mode) { throw "routing_mode is missing from settings API" }
    if ($null -eq $script:SettingsSnapshot.conversation_sticky) { throw "conversation_sticky is missing from settings API" }

    $targets = @($TargetAccountNames | ForEach-Object { Get-AccountByName $_ })
    if (@($targets).Count -lt 2) { throw "at least two target accounts are required" }
    Write-Info "target accounts: $(@($targets | ForEach-Object { $_.name }) -join ', ')"
    Write-Info "startup mode=$($script:SettingsSnapshot.routing_mode) conversation_sticky=$($script:SettingsSnapshot.conversation_sticky)"

    if (-not $SkipProtocolMatrix -and -not $OnlyConcurrency) {
        Run-Scenario "protocol matrix" { Invoke-ProtocolMatrix }
    }

    $targetIds = @($targets | ForEach-Object { $_.id })
    $otherIds = @($script:AccountsSnapshot | Where-Object { $targetIds -notcontains $_.id } | ForEach-Object { $_.id })
    $targetOrder = @($targetIds + $otherIds)

    if (-not $OnlyConcurrency) {
    Run-Scenario "strict priority follows order" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "strict-priority" $false
        $results = @(1..3 | ForEach-Object { Invoke-AndResolve "strict-$_" })
        Assert-Sequence @($results | ForEach-Object { $_.Log.Account }) @($targets[0].name, $targets[0].name, $targets[0].name) "strict priority sequence"
        $reverseOrder = @($targets[1].id, $targets[0].id) + $otherIds
        Set-AccountOrder $reverseOrder
        $reversed = @(1..2 | ForEach-Object { Invoke-AndResolve "strict-reversed-$_" })
        Assert-Sequence @($reversed | ForEach-Object { $_.Log.Account }) @($targets[1].name, $targets[1].name) "strict priority reversed sequence"
        Set-AccountOrder $targetOrder
    }

    Run-Scenario "global sticky keeps failover account" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "sticky-global" $false
        $initial = Invoke-AndResolve "global-initial"
        Assert-Equal $initial.Log.Account $targets[0].name "global initial account"
        Set-AccountEnabled $targets[0].id $false
        $failedOver = Invoke-AndResolve "global-failover"
        Assert-Equal $failedOver.Log.Account $targets[1].name "global failover account"
        Set-AccountEnabled $targets[0].id $true
        $recovered = @(1..2 | ForEach-Object { Invoke-AndResolve "global-recovered-$_" })
        Assert-Sequence @($recovered | ForEach-Object { $_.Log.Account }) @($targets[1].name, $targets[1].name) "global sticky after recovery"
    }

    Run-Scenario "round robin cycles and skips disabled account" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "round-robin" $false
        $cycled = @(1..6 | ForEach-Object { Invoke-AndResolve "round-robin-$_" })
        Assert-Sequence @($cycled | ForEach-Object { $_.Log.Account }) @($targets[0].name, $targets[1].name, $targets[0].name, $targets[1].name, $targets[0].name, $targets[1].name) "round robin sequence"
        Set-AccountEnabled $targets[1].id $false
        $single = @(1..3 | ForEach-Object { Invoke-AndResolve "round-robin-single-$_" })
        Assert-Sequence @($single | ForEach-Object { $_.Log.Account }) @($targets[0].name, $targets[0].name, $targets[0].name) "round robin disabled skip"
        Set-AccountEnabled $targets[1].id $true
    }

    Run-Scenario "conversation sticky keeps explicit bindings" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "round-robin" $true
        $a = @(1..3 | ForEach-Object { Invoke-AndResolve "conversation-a-$_" "live-routing-a" })
        $b = @(1..3 | ForEach-Object { Invoke-AndResolve "conversation-b-$_" "live-routing-b" })
        if ((@($a | ForEach-Object { $_.Log.Account }) | Select-Object -Unique).Count -ne 1) { throw "conversation A moved accounts" }
        if ((@($b | ForEach-Object { $_.Log.Account }) | Select-Object -Unique).Count -ne 1) { throw "conversation B moved accounts" }
        if ($a[0].Log.Account -eq $b[0].Log.Account) { throw "different conversation IDs shared one account unexpectedly" }
    }

    Run-Scenario "conversation sticky rebinds after disable" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "strict-priority" $true
        $first = Invoke-AndResolve "conversation-rebind-before" "live-routing-rebind"
        Set-AccountEnabled (($targets | Where-Object { $_.name -eq $first.Log.Account }).id) $false
        $second = Invoke-AndResolve "conversation-rebind-after" "live-routing-rebind"
        if ($second.Log.Account -eq $first.Log.Account) { throw "conversation binding did not fail over" }
        Set-AccountEnabled (($targets | Where-Object { $_.name -eq $first.Log.Account }).id) $true
        $third = Invoke-AndResolve "conversation-rebind-stable" "live-routing-rebind"
        Assert-Equal $third.Log.Account $second.Log.Account "conversation binding after failover"
    }

    Run-Scenario "prompt fingerprint ignores mutable history" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "round-robin" $true
        $first = Invoke-AndResolve "fingerprint-first" $null "stable-system" "first user" -UseTools
        $second = Invoke-AndResolve "fingerprint-followup" $null "stable-system" "first user" -UseTools -History @(
            @{ role = "assistant"; content = "previous answer" }
            @{ role = "user"; content = "latest user" }
        )
        Assert-Equal $second.Log.Account $first.Log.Account "prompt fingerprint account binding"
    }

    Run-Scenario "routing logs omit conversation and prompt source" {
        Assert-NoSensitiveRoutingLogData
    }

    Run-Scenario "model control requests do not advance generation cursor" {
        Set-AccountOrder $targetOrder
        Set-AccountEnabled $targets[0].id $true
        Set-AccountEnabled $targets[1].id $true
        Set-Routing "round-robin" $false
        $first = Invoke-AndResolve "control-generation-1"
        [void](Invoke-JsonRequest -Uri "$BaseUrl/v1/models" -Headers @{ Authorization = "Bearer $script:GatewayKey" })
        $second = Invoke-AndResolve "control-generation-2"
        [void](Invoke-JsonRequest -Uri "$BaseUrl/dashboard/api/application-models")
        $third = Invoke-AndResolve "control-generation-3"
        Assert-Sequence @($first.Log.Account, $second.Log.Account, $third.Log.Account) @($targets[0].name, $targets[1].name, $targets[0].name) "control request isolation sequence"
    }
    }

    if (-not $SkipConcurrency) {
        Run-Scenario "round robin concurrent requests" {
            Set-AccountOrder $targetOrder
            Set-AccountEnabled $targets[0].id $true
            Set-AccountEnabled $targets[1].id $true
            Set-Routing "round-robin" $false
            $jobs = @()
            1..8 | ForEach-Object {
                $jobs += Start-Job -ArgumentList $BaseUrl, $Model, $script:GatewayKey, $_ -ScriptBlock {
                    param($jobBaseUrl, $jobModel, $jobKey, $index)
                    $payload = @{ model = $jobModel; messages = @(@{ role = "user"; content = "reply with exactly: ok" }); max_tokens = 8; stream = $false } | ConvertTo-Json -Depth 8 -Compress
                    try {
                        $response = Invoke-WebRequest -Uri "$jobBaseUrl/v1/chat/completions" -Method POST -Headers @{ Authorization = "Bearer $jobKey"; "Content-Type" = "application/json" } -Body $payload -TimeoutSec 120
                        [pscustomobject]@{ Index = $index; Status = [int]$response.StatusCode; RequestId = [string]$response.Headers["x-ocg-request-id"]; Error = "" }
                    } catch {
                        [pscustomobject]@{ Index = $index; Status = 0; RequestId = ""; Error = $_.Exception.Message }
                    }
                }
            }
            $jobResults = @($jobs | Wait-Job | Receive-Job)
            $jobs | Remove-Job -Force
            if (@($jobResults | Where-Object { $_.Status -ne 200 }).Count -gt 0) { throw "concurrent request failed: $(($jobResults | Where-Object { $_.Status -ne 200 } | Out-String))" }
            if (@($jobResults | Where-Object { -not $_.RequestId }).Count -gt 0) { throw "concurrent response missing request id" }
            $accounts = @($jobResults | ForEach-Object {
                $log = Resolve-LogAccount ([pscustomobject]@{ RequestId = $_.RequestId })
                $log.Account
            })
            $firstCount = @($accounts | Where-Object { $_ -eq $targets[0].name }).Count
            $secondCount = @($accounts | Where-Object { $_ -eq $targets[1].name }).Count
            Assert-Equal $firstCount 4 "concurrent round-robin first account count"
            Assert-Equal $secondCount 4 "concurrent round-robin second account count"
            Write-Info "concurrent requests succeeded: total=$(@($jobResults).Count) distribution=$($targets[0].name):$firstCount,$($targets[1].name):$secondCount"
        }

        Run-Scenario "conversation sticky concurrent requests" {
            Set-AccountOrder $targetOrder
            Set-AccountEnabled $targets[0].id $true
            Set-AccountEnabled $targets[1].id $true
            Set-Routing "round-robin" $true
            $jobs = @()
            1..6 | ForEach-Object {
                $jobs += Start-Job -ArgumentList $BaseUrl, $Model, $script:GatewayKey, $_ -ScriptBlock {
                    param($jobBaseUrl, $jobModel, $jobKey, $index)
                    $payload = @{ model = $jobModel; messages = @(@{ role = "user"; content = "reply with exactly: ok" }); max_tokens = 8; stream = $false } | ConvertTo-Json -Depth 8 -Compress
                    try {
                        $response = Invoke-WebRequest -Uri "$jobBaseUrl/v1/chat/completions" -Method POST -Headers @{ Authorization = "Bearer $jobKey"; "Content-Type" = "application/json"; "X-OCG-Conversation-Id" = "live-routing-concurrent" } -Body $payload -TimeoutSec 120
                        [pscustomobject]@{ Index = $index; Status = [int]$response.StatusCode; RequestId = [string]$response.Headers["x-ocg-request-id"]; Error = "" }
                    } catch {
                        [pscustomobject]@{ Index = $index; Status = 0; RequestId = ""; Error = $_.Exception.Message }
                    }
                }
            }
            $jobResults = @($jobs | Wait-Job | Receive-Job)
            $jobs | Remove-Job -Force
            if (@($jobResults | Where-Object { $_.Status -ne 200 }).Count -gt 0) { throw "concurrent sticky request failed: $(($jobResults | Where-Object { $_.Status -ne 200 } | Out-String))" }
            $accounts = @($jobResults | ForEach-Object {
                $log = Resolve-LogAccount ([pscustomobject]@{ RequestId = $_.RequestId })
                $log.Account
            })
            $uniqueAccounts = @($accounts | Select-Object -Unique)
            Assert-Equal $uniqueAccounts.Count 1 "same conversation concurrent account count"
            Write-Info "same conversation concurrent requests stayed on account=$($uniqueAccounts[0])"
        }
    }
} catch {
    $script:FatalError = $_.Exception.Message
    Write-Result "FAIL" $_.Exception.Message
} finally {
    Restore-State
    $script:GatewayKey = $null
}

Write-Host "`n=== routing live test summary ==="
$script:ScenarioResults | Format-Table -AutoSize | Out-String | Write-Host
Write-Host "requests=$($script:RequestResults.Count) started=$($script:TestStartedAt.ToString('o'))"
if ($script:RestoreErrors.Count -gt 0) {
    Write-Result "FAIL" "restoration errors: $($script:RestoreErrors -join '; ')"
    exit 2
}
if ($script:FatalError) { exit 1 }
if (@($script:ScenarioResults | Where-Object { $_.Status -eq "FAIL" }).Count -gt 0) { exit 1 }
Write-Result "PASS" "all requested routing scenarios completed and state was restored"
