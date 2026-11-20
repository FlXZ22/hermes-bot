import React, { useState } from 'react';
import { createRoot } from 'react-dom/client';
import '@grok-bot/theme/styles';
import { ChatTranscriptView } from '../src/components/ChatTranscript';
import type { HermesMessage } from '@grok-bot/hermes-client';
const initial: HermesMessage[] = Array.from({ length: 1000 }, (_, i) => ({ id: `m-${i}`, role: i % 2 ? 'assistant' : 'user', content: `Message ${i}\n\n` + 'A paragraph for testing variable height chat rendering. '.repeat(i % 7 + 1) }));
const noop = async () => {};
function Fixture() {
 const [messages, setMessages] = useState(initial);
 Object.assign(window, { streaming: () => setMessages(prev => [...prev.slice(0,-1), {...prev.at(-1)!, streaming: true}]), append: (delta: string) => setMessages(prev => [...prev.slice(0,-1), { ...prev.at(-1)!, content: prev.at(-1)!.content + delta }]), finish: () => setMessages(prev => prev.map(m => ({...m, streaming: false}))), replaceLast: (content: string) => setMessages(prev => [...prev.slice(0,-1), {...prev.at(-1)!, content, thinking: '**Reasoning**\n\n- Check evidence', toolCalls: [{id:'tool', name:'terminal', status:'running', arguments:{command:'pwd'}, output:'{"ok":true}'}]}]) });
 return <div className="gb-chat" style={{height:'100vh'}}><ChatTranscriptView messages={messages} todos={[]} isStreaming={false} liveStatusText="" activeProfile="default" activeSession={null} botAvatars={{}} respondApproval={noop} respondClarify={noop} /></div>;
}
createRoot(document.getElementById('root')!).render(<Fixture />);
