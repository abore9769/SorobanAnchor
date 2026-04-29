/**
 * Webhook Monitor Integration Example
 * 
 * This file shows how to integrate the webhook monitor with your backend.
 * Choose one of the three methods below based on your infrastructure.
 */

// ============================================================================
// METHOD 1: WebSocket (Recommended for Real-Time)
// ============================================================================

class WebhookMonitorWebSocket {
    constructor(wsUrl) {
        this.wsUrl = wsUrl;
        this.ws = null;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 5;
    }

    connect() {
        this.ws = new WebSocket(this.wsUrl);

        this.ws.onopen = () => {
            console.log('✅ WebSocket connected');
            this.reconnectAttempts = 0;
        };

        this.ws.onmessage = (event) => {
            try {
                const webhookData = JSON.parse(event.data);
                this.handleWebhookEvent(webhookData);
            } catch (error) {
                console.error('Failed to parse webhook event:', error);
            }
        };

        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
        };

        this.ws.onclose = () => {
            console.log('WebSocket closed');
            this.attemptReconnect();
        };
    }

    attemptReconnect() {
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.reconnectAttempts++;
            const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 30000);
            console.log(`Reconnecting in ${delay}ms... (attempt ${this.reconnectAttempts})`);
            setTimeout(() => this.connect(), delay);
        }
    }

    handleWebhookEvent(webhookData) {
        // Add to monitor (assumes addEvent function exists in webhook_monitor.html)
        if (typeof addEvent === 'function') {
            addEvent({
                id: ++eventCounter,
                type: webhookData.type,
                timestamp: webhookData.timestamp || new Date().toISOString(),
                payload: webhookData.payload
            });
        }
    }

    disconnect() {
        if (this.ws) {
            this.ws.close();
            this.ws = null;
        }
    }
}

// Usage:
// const monitor = new WebhookMonitorWebSocket('ws://localhost:8080/webhooks');
// monitor.connect();

// ============================================================================
// METHOD 2: Server-Sent Events (SSE)
// ============================================================================

class WebhookMonitorSSE {
    constructor(sseUrl) {
        this.sseUrl = sseUrl;
        this.eventSource = null;
    }

    connect() {
        this.eventSource = new EventSource(this.sseUrl);

        this.eventSource.onopen = () => {
            console.log('✅ SSE connected');
        };

        this.eventSource.onmessage = (event) => {
            try {
                const webhookData = JSON.parse(event.data);
                this.handleWebhookEvent(webhookData);
            } catch (error) {
                console.error('Failed to parse SSE event:', error);
            }
        };

        this.eventSource.onerror = (error) => {
            console.error('SSE error:', error);
            // SSE automatically reconnects
        };
    }

    handleWebhookEvent(webhookData) {
        if (typeof addEvent === 'function') {
            addEvent({
                id: ++eventCounter,
                type: webhookData.type,
                timestamp: webhookData.timestamp || new Date().toISOString(),
                payload: webhookData.payload
            });
        }
    }

    disconnect() {
        if (this.eventSource) {
            this.eventSource.close();
            this.eventSource = null;
        }
    }
}

// Usage:
// const monitor = new WebhookMonitorSSE('/api/webhook-stream');
// monitor.connect();

// ============================================================================
// METHOD 3: HTTP Polling (Fallback)
// ============================================================================

class WebhookMonitorPolling {
    constructor(apiUrl, intervalMs = 2000) {
        this.apiUrl = apiUrl;
        this.intervalMs = intervalMs;
        this.intervalId = null;
        this.lastEventId = 0;
    }

    start() {
        this.intervalId = setInterval(() => this.poll(), this.intervalMs);
        this.poll(); // Initial poll
    }

    async poll() {
        try {
            const response = await fetch(`${this.apiUrl}?since=${this.lastEventId}`);
            
            if (!response.ok) {
                throw new Error(`HTTP ${response.status}: ${response.statusText}`);
            }

            const events = await response.json();
            
            events.forEach(webhookData => {
                this.handleWebhookEvent(webhookData);
                if (webhookData.id > this.lastEventId) {
                    this.lastEventId = webhookData.id;
                }
            });
        } catch (error) {
            console.error('Polling error:', error);
        }
    }

    handleWebhookEvent(webhookData) {
        if (typeof addEvent === 'function') {
            addEvent({
                id: ++eventCounter,
                type: webhookData.type,
                timestamp: webhookData.timestamp || new Date().toISOString(),
                payload: webhookData.payload
            });
        }
    }

    stop() {
        if (this.intervalId) {
            clearInterval(this.intervalId);
            this.intervalId = null;
        }
    }
}

// Usage:
// const monitor = new WebhookMonitorPolling('/api/webhooks/recent', 3000);
// monitor.start();

// ============================================================================
// BACKEND EXAMPLES
// ============================================================================

// Express.js + WebSocket Example
/*
const express = require('express');
const WebSocket = require('ws');
const app = express();

const wss = new WebSocket.Server({ port: 8080 });

// Webhook endpoint
app.post('/webhook', express.json(), (req, res) => {
    const event = {
        id: Date.now(),
        type: req.body.type,
        timestamp: new Date().toISOString(),
        payload: req.body.data
    };
    
    // Broadcast to all connected clients
    wss.clients.forEach(client => {
        if (client.readyState === WebSocket.OPEN) {
            client.send(JSON.stringify(event));
        }
    });
    
    res.status(200).json({ success: true });
});

app.listen(3000, () => console.log('Server running on port 3000'));
*/

// Express.js + SSE Example
/*
const express = require('express');
const app = express();

const clients = [];

// SSE endpoint
app.get('/api/webhook-stream', (req, res) => {
    res.setHeader('Content-Type', 'text/event-stream');
    res.setHeader('Cache-Control', 'no-cache');
    res.setHeader('Connection', 'keep-alive');
    
    clients.push(res);
    
    req.on('close', () => {
        clients.splice(clients.indexOf(res), 1);
    });
});

// Webhook endpoint
app.post('/webhook', express.json(), (req, res) => {
    const event = {
        id: Date.now(),
        type: req.body.type,
        timestamp: new Date().toISOString(),
        payload: req.body.data
    };
    
    // Send to all SSE clients
    clients.forEach(client => {
        client.write(`data: ${JSON.stringify(event)}\n\n`);
    });
    
    res.status(200).json({ success: true });
});

app.listen(3000, () => console.log('Server running on port 3000'));
*/

// ============================================================================
// SECURITY MIDDLEWARE EXAMPLE
// ============================================================================

function sanitizeWebhookPayload(payload) {
    const sanitized = { ...payload };
    
    // Redact email addresses
    if (sanitized.email) {
        const [local, domain] = sanitized.email.split('@');
        sanitized.email = `${local.substring(0, 2)}***@${domain}`;
    }
    
    // Truncate addresses
    if (sanitized.user) {
        sanitized.user = `${sanitized.user.substring(0, 8)}...${sanitized.user.substring(sanitized.user.length - 3)}`;
    }
    
    // Remove sensitive fields
    delete sanitized.password;
    delete sanitized.apiKey;
    delete sanitized.secret;
    
    return sanitized;
}

// ============================================================================
// FILTERING API USAGE
// ============================================================================

// The webhook monitor exposes several filtering functions that can be used
// programmatically to filter events based on various criteria.

class WebhookFilterAPI {
    constructor() {
        this.events = [];
    }

    // Add event to the filterable list
    addEvent(event) {
        this.events.unshift(event);
    }

    // Filter events by type
    filterByType(type) {
        if (type === 'all') return this.events;
        return this.events.filter(e => e.type === type);
    }

    // Filter events by anchor
    filterByAnchor(anchor) {
        if (anchor === 'all') return this.events;
        return this.events.filter(e => e.payload.anchor === anchor);
    }

    // Filter events by status
    filterByStatus(status) {
        if (status === 'all') return this.events;
        return this.events.filter(e => e.payload.status === status);
    }

    // Filter events by date range
    filterByDateRange(from, to) {
        return this.events.filter(event => {
            const eventDate = new Date(event.timestamp);
            if (from && eventDate < new Date(from)) return false;
            if (to && eventDate > new Date(to)) return false;
            return true;
        });
    }

    // Search by transaction ID or request ID
    search(query) {
        const lowerQuery = query.toLowerCase();
        return this.events.filter(event => {
            const transactionId = event.payload.transaction_id || '';
            const requestId = event.payload.request_id || '';
            return transactionId.toLowerCase().includes(lowerQuery) ||
                   requestId.toLowerCase().includes(lowerQuery);
        });
    }

    // Combined filter - apply all filters at once
    applyFilters(options = {}) {
        let filtered = [...this.events];

        if (options.type && options.type !== 'all') {
            filtered = filtered.filter(e => e.type === options.type);
        }

        if (options.anchor && options.anchor !== 'all') {
            filtered = filtered.filter(e => e.payload.anchor === options.anchor);
        }

        if (options.status && options.status !== 'all') {
            filtered = filtered.filter(e => e.payload.status === options.status);
        }

        if (options.from) {
            filtered = filtered.filter(e => new Date(e.timestamp) >= new Date(options.from));
        }

        if (options.to) {
            filtered = filtered.filter(e => new Date(e.timestamp) <= new Date(options.to));
        }

        if (options.search) {
            const query = options.search.toLowerCase();
            filtered = filtered.filter(e => {
                const txId = e.payload.transaction_id || '';
                const reqId = e.payload.request_id || '';
                return txId.toLowerCase().includes(query) || reqId.toLowerCase().includes(query);
            });
        }

        return filtered;
    }

    // Export filtered events to JSON
    exportToJSON(filteredEvents) {
        return JSON.stringify(filteredEvents, null, 2);
    }

    // Export filtered events to CSV
    exportToCSV(filteredEvents) {
        const headers = ['ID', 'Type', 'Timestamp', 'Anchor', 'Status', 'Transaction ID', 'Request ID'];
        const rows = filteredEvents.map(event => [
            event.id,
            event.type,
            event.timestamp,
            event.payload.anchor || '',
            event.payload.status || '',
            event.payload.transaction_id || '',
            event.payload.request_id || ''
        ]);

        return [headers.join(','), ...rows.map(row => row.join(','))].join('\n');
    }
}

// Usage example:
/*
const filterAPI = new WebhookFilterAPI();

// Add some sample events
filterAPI.addEvent({ id: 1, type: 'deposit', timestamp: '2024-01-15T10:00:00Z', payload: { anchor: 'example-anchor', status: 'success', transaction_id: 'tx_001' } });
filterAPI.addEvent({ id: 2, type: 'withdrawal', timestamp: '2024-01-15T11:00:00Z', payload: { anchor: 'stablecoin', status: 'pending', transaction_id: 'tx_002' } });
filterAPI.addEvent({ id: 3, type: 'kyc', timestamp: '2024-01-15T12:00:00Z', payload: { anchor: 'fiat-ramp', status: 'success', request_id: 'req_001' } });

// Filter by type
const deposits = filterAPI.filterByType('deposit');
console.log('Deposits:', deposits);

// Filter by anchor
const anchorEvents = filterAPI.filterByAnchor('example-anchor');
console.log('Example Anchor events:', anchorEvents);

// Combined filter
const filtered = filterAPI.applyFilters({
    type: 'deposit',
    status: 'success',
    from: '2024-01-01',
    to: '2024-12-31'
});
console.log('Filtered events:', filtered);

// Export to JSON
const jsonData = filterAPI.exportToJSON(filtered);
console.log('JSON Export:', jsonData);

// Export to CSV
const csvData = filterAPI.exportToCSV(filtered);
console.log('CSV Export:', csvData);
*/

// ============================================================================
// URL STATE MANAGEMENT
// ============================================================================

// The filter state can be preserved in the URL for bookmarking and sharing

class WebhookFilterURLManager {
    // Update URL with current filter state
    static updateURL(filters) {
        const params = new URLSearchParams();
        
        if (filters.type && filters.type !== 'all') params.set('type', filters.type);
        if (filters.anchor && filters.anchor !== 'all') params.set('anchor', filters.anchor);
        if (filters.status && filters.status !== 'all') params.set('status', filters.status);
        if (filters.from) params.set('from', filters.from);
        if (filters.to) params.set('to', filters.to);
        if (filters.search) params.set('search', filters.search);

        const newURL = window.location.pathname + (params.toString() ? '?' + params.toString() : '');
        window.history.replaceState({}, '', newURL);
    }

    // Load filter state from URL
    static loadFromURL() {
        const params = new URLSearchParams(window.location.search);
        
        return {
            type: params.get('type') || 'all',
            anchor: params.get('anchor') || 'all',
            status: params.get('status') || 'all',
            from: params.get('from') || '',
            to: params.get('to') || '',
            search: params.get('search') || ''
        };
    }

    // Get shareable URL with current filters
    static getShareableURL(filters) {
        this.updateURL(filters);
        return window.location.href;
    }
}

// Usage:
/*
// Save current filters to URL
const currentFilters = {
    type: 'deposit',
    anchor: 'example-anchor',
    status: 'success',
    from: '2024-01-01',
    to: '2024-12-31',
    search: 'tx_001'
};
WebhookFilterURLManager.updateURL(currentFilters);

// Load filters from URL
const savedFilters = WebhookFilterURLManager.loadFromURL();
console.log('Loaded filters:', savedFilters);

// Get shareable URL
const shareURL = WebhookFilterURLManager.getShareableURL(currentFilters);
console.log('Share this URL:', shareURL);
*/

// ============================================================================
// STELLAR HORIZON INTEGRATION EXAMPLE
// ============================================================================

/*
const StellarSdk = require('stellar-sdk');

function monitorStellarAccount(accountId) {
    const server = new StellarSdk.Server('https://horizon-testnet.stellar.org');
    
    server.operations()
        .forAccount(accountId)
        .cursor('now')
        .stream({
            onmessage: (operation) => {
                const event = {
                    id: operation.id,
                    type: mapOperationType(operation.type),
                    timestamp: operation.created_at,
                    payload: {
                        operation_type: operation.type,
                        from: operation.from,
                        to: operation.to,
                        amount: operation.amount,
                        asset: operation.asset_type
                    }
                };
                
                // Send to webhook monitor
                broadcastEvent(event);
            },
            onerror: (error) => {
                console.error('Stellar stream error:', error);
            }
        });
}

function mapOperationType(stellarType) {
    const typeMap = {
        'payment': 'transfer',
        'create_account': 'deposit',
        'path_payment_strict_receive': 'transfer',
        'path_payment_strict_send': 'transfer'
    };
    return typeMap[stellarType] || 'other';
}
*/

// ============================================================================
// USAGE IN webhook_monitor.html
// ============================================================================

/*
Replace the simulation code in webhook_monitor.html with:

<script>
    // Choose your integration method
    const monitor = new WebhookMonitorWebSocket('ws://localhost:8080/webhooks');
    // OR
    // const monitor = new WebhookMonitorSSE('/api/webhook-stream');
    // OR
    // const monitor = new WebhookMonitorPolling('/api/webhooks/recent', 3000);
    
    // Start monitoring
    monitor.connect(); // or monitor.start() for polling
    
    // Clean up on page unload
    window.addEventListener('beforeunload', () => {
        monitor.disconnect(); // or monitor.stop() for polling
    });
</script>
*/
