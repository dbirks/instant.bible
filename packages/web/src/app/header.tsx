import * as React from 'react';
import { useDispatch } from 'react-redux';
import { t } from '@lingui/macro';
import { css } from '@emotion/core';
import { I18n } from '@lingui/react';
import { Logo, Input } from '../elements';
import { doSearch, doReset, useDirty, useQuery } from '../state/search';
import styled, { ThemedFn } from '../util/styled';

const getBackgroundColor: ThemedFn = ({ theme }) => theme.background;

const Root = styled('header')<{ dirty: boolean }>`
  background: ${getBackgroundColor};
  width: ${({ dirty }) => (dirty ? '960px' : '40%')};
  display: flex;
  flex-direction: ${({ dirty }) => (dirty ? 'row' : 'column')};
  align-items: center;
  justify-content: center;
  padding-top: ${({ dirty }) => (dirty ? '15px' : '0')};
  padding-bottom: ${({ dirty }) => (dirty ? '15px' : '35vh')};
  position: ${({ dirty }) => (dirty ? 'fixed' : null)};
  z-index: 1;
`;

export default React.memo(() => {
  const inputRef = React.useRef<HTMLInputElement>(null);
  const dispatch = useDispatch();
  const query = useQuery();

  const handleChange = React.useCallback(
    (e: React.FormEvent<HTMLInputElement>) => {
      dispatch(doSearch(e.currentTarget.value));
    },
    [dispatch],
  );

  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!inputRef.current) {
        return;
      }

      if (e.target !== inputRef.current && e.key === '/') {
        e.preventDefault();
        inputRef.current.focus();
      }
    };

    document.addEventListener('keyup', handler);

    return () => {
      document.removeEventListener('keyup', handler);
    };
  }, [inputRef]);

  React.useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        dispatch(doReset());
      }
    };

    document.addEventListener('keyup', handler);

    return () => {
      document.removeEventListener('keyup', handler);
    };
  }, [dispatch]);

  const dirty = useDirty();

  return (
    <I18n>
      {({ i18n }): React.ReactElement => (
        <Root dirty={dirty}>
          <div
            css={css`
              width: ${dirty ? '200px' : '50%'};
            `}
          >
            <Logo />
          </div>
          <Input
            ref={inputRef}
            css={css`
              width: 100%;
              margin-top: ${dirty ? 'none' : '30px'};
              margin-left: ${dirty ? '15px' : 'none'};
            `}
            placeholder={i18n._(t`Search...`)}
            onChange={handleChange}
            value={query}
            autoFocus={true}
          />
        </Root>
      )}
    </I18n>
  );
});
