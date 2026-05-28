-- Add thread_id and topic_name columns to channel_messages for Telegram forum topic awareness
-- thread_id: the root message ID of the forum topic (Telegram's message_thread_id)
-- topic_name: human-readable name of the topic (resolved once and cached)

ALTER TABLE channel_messages ADD COLUMN thread_id TEXT;
ALTER TABLE channel_messages ADD COLUMN topic_name TEXT;

-- Index for filtering by thread within a chat
CREATE INDEX IF NOT EXISTS idx_channel_messages_thread ON channel_messages(channel, channel_chat_id, thread_id);
