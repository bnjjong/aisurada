// 아키텍처의 이름을 받는 작은 창(새로 만들기와 이름 바꾸기가 같이 쓴다). 길이와 중복 검사는 엔진이 하고, 여기서는 이유만 보여 준다.
import { useState } from 'react';
import { Modal } from '../Modal.tsx';

export function DiagramNameModal({ title, initial = '', submit, close }: {
  title: string;
  initial?: string;
  submit: (name: string) => Promise<string | null>;
  close(): void;
}) {
  const [name, setName] = useState(initial);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const onSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    setBusy(true); setError('');
    const failure = await submit(name.trim());
    if (failure === null) close();
    else { setError(failure); setBusy(false); }
  };
  return (
    <Modal title={title} close={close} className="ndf c4-form" resizable={false}>
      <form onSubmit={event => void onSubmit(event)}>
        <label>이름
          <input required autoFocus maxLength={60} value={name} placeholder="예: 결제 흐름" onChange={event => setName(event.target.value)} />
        </label>
        {error && <p className="c4-error" role="alert">{error}</p>}
        <footer>
          <button type="button" onClick={close}>취소</button>
          <button type="submit" className="button-primary" disabled={busy || !name.trim() || name.trim() === initial}>{initial ? '바꾸기' : '만들기'}</button>
        </footer>
      </form>
    </Modal>
  );
}
