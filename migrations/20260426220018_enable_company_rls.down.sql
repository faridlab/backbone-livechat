-- Down: remove the company RLS fence for livechat module

-- Reverse the company RLS fence for livechat.channels
DROP POLICY IF EXISTS channels_company_isolation ON livechat.channels;
ALTER TABLE livechat.channels NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.channels DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.channel_members
DROP POLICY IF EXISTS channel_members_company_isolation ON livechat.channel_members;
ALTER TABLE livechat.channel_members NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.channel_members DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.channel_rules
DROP POLICY IF EXISTS channel_rules_company_isolation ON livechat.channel_rules;
ALTER TABLE livechat.channel_rules NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.channel_rules DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.chatbot_answers
DROP POLICY IF EXISTS chatbot_answers_company_isolation ON livechat.chatbot_answers;
ALTER TABLE livechat.chatbot_answers NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.chatbot_answers DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.chatbot_messages
DROP POLICY IF EXISTS chatbot_messages_company_isolation ON livechat.chatbot_messages;
ALTER TABLE livechat.chatbot_messages NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.chatbot_messages DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.chatbot_scripts
DROP POLICY IF EXISTS chatbot_scripts_company_isolation ON livechat.chatbot_scripts;
ALTER TABLE livechat.chatbot_scripts NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.chatbot_scripts DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.chatbot_steps
DROP POLICY IF EXISTS chatbot_steps_company_isolation ON livechat.chatbot_steps;
ALTER TABLE livechat.chatbot_steps NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.chatbot_steps DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.chatbot_step_triggers
DROP POLICY IF EXISTS chatbot_step_triggers_company_isolation ON livechat.chatbot_step_triggers;
ALTER TABLE livechat.chatbot_step_triggers NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.chatbot_step_triggers DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.conversation_tags
DROP POLICY IF EXISTS conversation_tags_company_isolation ON livechat.conversation_tags;
ALTER TABLE livechat.conversation_tags NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.conversation_tags DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.expertise_tags
DROP POLICY IF EXISTS expertise_tags_company_isolation ON livechat.expertise_tags;
ALTER TABLE livechat.expertise_tags NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.expertise_tags DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.livechat_audit_log
DROP POLICY IF EXISTS livechat_audit_log_company_isolation ON livechat.livechat_audit_log;
ALTER TABLE livechat.livechat_audit_log NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.livechat_audit_log DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.member_histories
DROP POLICY IF EXISTS member_histories_company_isolation ON livechat.member_histories;
ALTER TABLE livechat.member_histories NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.member_histories DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.operator_expertise
DROP POLICY IF EXISTS operator_expertise_company_isolation ON livechat.operator_expertise;
ALTER TABLE livechat.operator_expertise NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.operator_expertise DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.operator_profiles
DROP POLICY IF EXISTS operator_profiles_company_isolation ON livechat.operator_profiles;
ALTER TABLE livechat.operator_profiles NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.operator_profiles DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.ratings
DROP POLICY IF EXISTS ratings_company_isolation ON livechat.ratings;
ALTER TABLE livechat.ratings NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.ratings DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.sessions
DROP POLICY IF EXISTS sessions_company_isolation ON livechat.sessions;
ALTER TABLE livechat.sessions NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.sessions DISABLE ROW LEVEL SECURITY;

-- Reverse the company RLS fence for livechat.session_tags
DROP POLICY IF EXISTS session_tags_company_isolation ON livechat.session_tags;
ALTER TABLE livechat.session_tags NO FORCE ROW LEVEL SECURITY;
ALTER TABLE livechat.session_tags DISABLE ROW LEVEL SECURITY;

