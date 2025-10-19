/*******************************************************************************
 *
 *    Copyright (c) 2025.
 *    3-Prism Co. Ltd.
 *
 *    All rights reserved.
 *
 ******************************************************************************/
//! # Retry Executor Tests
//!
//! 专门测试 RetryExecutor 内部方法的各种分支情况，确保所有条件分支都被覆盖。
//!
//! # Author
//!
//! 胡海星

use prism3_retry::{
    AbortEvent, FailureEvent, RetryBuilder, RetryDelayStrategy, RetryError, RetryEvent,
    SuccessEvent,
};
use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TestData {
    value: String,
}

#[derive(Debug)]
struct TestError(String);

impl std::fmt::Display for TestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TestError: {}", self.0)
    }
}

impl Error for TestError {}

// ==================== check_max_duration_exceeded() 测试 ====================

#[test]
fn test_check_max_duration_exceeded_with_none_max_duration() {
    // 测试 max_duration 为 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_max_duration(None) // max_duration 为 None
        .set_delay_strategy(RetryDelayStrategy::None)
        .build();

    let result = executor.run(|| Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string()));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[test]
fn test_check_max_duration_exceeded_with_some_max_duration_not_exceeded() {
    // 测试 max_duration 有值但未超时的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_max_duration(Some(Duration::from_secs(5))) // max_duration 有值
        .set_delay_strategy(RetryDelayStrategy::None)
        .build();

    let result = executor.run(|| {
        // 快速完成，不会超时
        Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[test]
fn test_check_max_duration_exceeded_with_some_max_duration_exceeded() {
    // 测试 max_duration 有值且超时的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(100)
        .set_max_duration(Some(Duration::from_millis(100))) // max_duration 有值且很短
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(50),
        })
        .build();

    let result = executor.run(|| {
        std::thread::sleep(Duration::from_millis(10));
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxDurationExceeded { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected MaxDurationExceeded error"),
    }
}

#[test]
fn test_check_max_duration_exceeded_with_none_failure_listener() {
    // 测试 max_duration 超时且 failure_listener 为 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(100)
        .set_max_duration(Some(Duration::from_millis(100)))
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(50),
        })
        // 没有设置 failure_listener
        .build();

    let result = executor.run(|| {
        std::thread::sleep(Duration::from_millis(10));
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxDurationExceeded { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected MaxDurationExceeded error"),
    }
}

#[test]
fn test_check_max_duration_exceeded_with_some_failure_listener() {
    // 测试 max_duration 超时且 failure_listener 有值的情况
    let failure_count = Arc::new(Mutex::new(0));
    let failure_count_clone = failure_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(100)
        .set_max_duration(Some(Duration::from_millis(100)))
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(50),
        })
        .on_failure(move |_event: &FailureEvent<String>| {
            *failure_count_clone.lock().unwrap() += 1;
        })
        .build();

    let result = executor.run(|| {
        std::thread::sleep(Duration::from_millis(10));
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxDurationExceeded { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected MaxDurationExceeded error"),
    }

    // 验证 failure_listener 被调用了
    assert_eq!(*failure_count.lock().unwrap(), 1);
}

// ==================== check_operation_timeout() 测试 ====================

#[test]
fn test_check_operation_timeout_with_none_timeout() {
    // 测试 operation_timeout 为 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_operation_timeout(None) // operation_timeout 为 None
        .build();

    let result = executor.run(|| {
        // 即使操作时间较长，也不会超时
        std::thread::sleep(Duration::from_millis(200));
        Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[test]
fn test_check_operation_timeout_with_some_timeout_not_exceeded() {
    // 测试 operation_timeout 有值但未超时的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_operation_timeout(Some(Duration::from_millis(200))) // operation_timeout 有值
        .build();

    let result = executor.run(|| {
        // 操作时间短于超时时间
        std::thread::sleep(Duration::from_millis(50));
        Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[test]
fn test_check_operation_timeout_with_some_timeout_exceeded() {
    // 测试 operation_timeout 有值且超时的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_operation_timeout(Some(Duration::from_millis(50))) // operation_timeout 有值且很短
        .set_delay_strategy(RetryDelayStrategy::None)
        .build();

    let result = executor.run(|| {
        // 操作时间长于超时时间
        std::thread::sleep(Duration::from_millis(150));
        Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
    });

    // 应该失败，因为每次操作都超时
    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }
}

// ==================== handle_success() 测试 ====================

#[test]
fn test_handle_success_with_none_listener() {
    // 测试 success_listener 为 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        // 没有设置 success_listener
        .build();

    let result = executor.run(|| Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string()));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[test]
fn test_handle_success_with_some_listener() {
    // 测试 success_listener 有值的情况
    let success_count = Arc::new(Mutex::new(0));
    let success_count_clone = success_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .on_success(move |event: &SuccessEvent<String>| {
            *success_count_clone.lock().unwrap() += 1;
            assert_eq!(event.result(), "SUCCESS");
        })
        .build();

    let result = executor.run(|| Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string()));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    // 验证 success_listener 被调用了
    assert_eq!(*success_count.lock().unwrap(), 1);
}

// ==================== handle_abort() 测试 ====================

#[test]
fn test_handle_abort_with_none_listener() {
    // 测试 abort_listener 为 None 的情况
    let abort_result = TestData {
        value: "ABORT".to_string(),
    };

    let executor = RetryBuilder::<TestData>::new()
        .set_max_attempts(3)
        .abort_on_results(vec![abort_result.clone()])
        // 没有设置 abort_listener
        .build();

    let result =
        executor.run(|| Ok::<TestData, Box<dyn Error + Send + Sync>>(abort_result.clone()));

    assert!(result.is_err());
    match result {
        Err(RetryError::Aborted { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected Aborted error"),
    }
}

#[test]
fn test_handle_abort_with_some_listener() {
    // 测试 abort_listener 有值的情况
    let abort_count = Arc::new(Mutex::new(0));
    let abort_count_clone = abort_count.clone();
    let abort_result = TestData {
        value: "ABORT".to_string(),
    };

    let executor = RetryBuilder::<TestData>::new()
        .set_max_attempts(3)
        .abort_on_results(vec![abort_result.clone()])
        .on_abort(move |_event: &AbortEvent<TestData>| {
            *abort_count_clone.lock().unwrap() += 1;
        })
        .build();

    let result =
        executor.run(|| Ok::<TestData, Box<dyn Error + Send + Sync>>(abort_result.clone()));

    assert!(result.is_err());
    match result {
        Err(RetryError::Aborted { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected Aborted error"),
    }

    // 验证 abort_listener 被调用了
    assert_eq!(*abort_count.lock().unwrap(), 1);
}

// ==================== handle_max_attempts_exceeded() 测试 ====================

#[test]
fn test_handle_max_attempts_exceeded_with_none_failure_listener() {
    // 测试达到最大重试次数且 failure_listener 为 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::None)
        // 没有设置 failure_listener
        .build();

    let result = executor.run(|| {
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { attempts, .. }) => {
            assert_eq!(attempts, 3);
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }
}

#[test]
fn test_handle_max_attempts_exceeded_with_some_failure_listener() {
    // 测试达到最大重试次数且 failure_listener 有值的情况
    let failure_count = Arc::new(Mutex::new(0));
    let failure_count_clone = failure_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::None)
        .on_failure(move |event: &FailureEvent<String>| {
            *failure_count_clone.lock().unwrap() += 1;
            assert_eq!(event.attempt_count(), 3);
        })
        .build();

    let result = executor.run(|| {
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { attempts, .. }) => {
            assert_eq!(attempts, 3);
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }

    // 验证 failure_listener 被调用了
    assert_eq!(*failure_count.lock().unwrap(), 1);
}

#[test]
fn test_handle_max_attempts_exceeded_with_result_failure() {
    // 测试因结果值失败达到最大重试次数的情况
    let failure_count = Arc::new(Mutex::new(0));
    let failure_count_clone = failure_count.clone();
    let failed_result = TestData {
        value: "FAIL".to_string(),
    };

    let executor = RetryBuilder::<TestData>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::None)
        .failed_on_results(vec![failed_result.clone()])
        .on_failure(move |event: &FailureEvent<TestData>| {
            *failure_count_clone.lock().unwrap() += 1;
            assert!(event.last_result().is_some());
            assert_eq!(event.last_result().unwrap().value, "FAIL");
        })
        .build();

    let result =
        executor.run(|| Ok::<TestData, Box<dyn Error + Send + Sync>>(failed_result.clone()));

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { attempts, .. }) => {
            assert_eq!(attempts, 3);
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }

    // 验证 failure_listener 被调用了
    assert_eq!(*failure_count.lock().unwrap(), 1);
}

// ==================== trigger_retry_and_wait() 测试 ====================

#[test]
fn test_trigger_retry_and_wait_with_none_listener() {
    // 测试 retry_listener 为 None 的情况
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(10),
        })
        // 没有设置 retry_listener
        .build();

    let result = executor.run(|| {
        let mut count = attempt_count_clone.lock().unwrap();
        *count += 1;
        let current = *count;
        drop(count);

        if current < 2 {
            Err(Box::new(TestError("Temporary failure".to_string()))
                as Box<dyn Error + Send + Sync>)
        } else {
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
}

#[test]
fn test_trigger_retry_and_wait_with_some_listener() {
    // 测试 retry_listener 有值的情况
    let retry_count = Arc::new(Mutex::new(0));
    let retry_count_clone = retry_count.clone();
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(10),
        })
        .on_retry(move |event: &RetryEvent<String>| {
            *retry_count_clone.lock().unwrap() += 1;
            assert!(event.attempt_count() > 0);
        })
        .build();

    let result = executor.run(|| {
        let mut count = attempt_count_clone.lock().unwrap();
        *count += 1;
        let current = *count;
        drop(count);

        if current < 2 {
            Err(Box::new(TestError("Temporary failure".to_string()))
                as Box<dyn Error + Send + Sync>)
        } else {
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
    // 验证 retry_listener 被调用了（第一次失败后触发重试）
    assert_eq!(*retry_count.lock().unwrap(), 1);
}

#[test]
fn test_trigger_retry_and_wait_with_zero_delay() {
    // 测试延迟为零的情况
    let retry_count = Arc::new(Mutex::new(0));
    let retry_count_clone = retry_count.clone();
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::None) // 零延迟
        .on_retry(move |_event: &RetryEvent<String>| {
            *retry_count_clone.lock().unwrap() += 1;
        })
        .build();

    let result = executor.run(|| {
        let mut count = attempt_count_clone.lock().unwrap();
        *count += 1;
        let current = *count;
        drop(count);

        if current < 2 {
            Err(Box::new(TestError("Temporary failure".to_string()))
                as Box<dyn Error + Send + Sync>)
        } else {
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
    assert_eq!(*retry_count.lock().unwrap(), 1);
}

// ==================== trigger_retry_and_wait_async() 测试 ====================

#[tokio::test]
async fn test_trigger_retry_and_wait_async_with_none_listener() {
    // 测试异步版本 retry_listener 为 None 的情况
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(10),
        })
        // 没有设置 retry_listener
        .build();

    let result = executor
        .run_async(|| async {
            let mut count = attempt_count_clone.lock().unwrap();
            *count += 1;
            let current = *count;
            drop(count);

            if current < 2 {
                Err(Box::new(TestError("Temporary failure".to_string()))
                    as Box<dyn Error + Send + Sync>)
            } else {
                Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
}

#[tokio::test]
async fn test_trigger_retry_and_wait_async_with_some_listener() {
    // 测试异步版本 retry_listener 有值的情况
    let retry_count = Arc::new(Mutex::new(0));
    let retry_count_clone = retry_count.clone();
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(10),
        })
        .on_retry(move |event: &RetryEvent<String>| {
            *retry_count_clone.lock().unwrap() += 1;
            assert!(event.attempt_count() > 0);
        })
        .build();

    let result = executor
        .run_async(|| async {
            let mut count = attempt_count_clone.lock().unwrap();
            *count += 1;
            let current = *count;
            drop(count);

            if current < 2 {
                Err(Box::new(TestError("Temporary failure".to_string()))
                    as Box<dyn Error + Send + Sync>)
            } else {
                Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
    // 验证 retry_listener 被调用了
    assert_eq!(*retry_count.lock().unwrap(), 1);
}

#[tokio::test]
async fn test_trigger_retry_and_wait_async_with_zero_delay() {
    // 测试异步版本延迟为零的情况
    let retry_count = Arc::new(Mutex::new(0));
    let retry_count_clone = retry_count.clone();
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::None) // 零延迟
        .on_retry(move |_event: &RetryEvent<String>| {
            *retry_count_clone.lock().unwrap() += 1;
        })
        .build();

    let result = executor
        .run_async(|| async {
            let mut count = attempt_count_clone.lock().unwrap();
            *count += 1;
            let current = *count;
            drop(count);

            if current < 2 {
                Err(Box::new(TestError("Temporary failure".to_string()))
                    as Box<dyn Error + Send + Sync>)
            } else {
                Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
    assert_eq!(*retry_count.lock().unwrap(), 1);
}

// ==================== execute_operation_async_and_get_decision() 测试 ====================

#[tokio::test]
async fn test_execute_operation_async_with_none_timeout() {
    // 测试异步操作 operation_timeout 为 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_operation_timeout(None) // operation_timeout 为 None
        .build();

    let result = executor
        .run_async(|| async {
            // 即使操作时间较长，也不会超时
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[tokio::test]
async fn test_execute_operation_async_with_some_timeout_not_exceeded() {
    // 测试异步操作 operation_timeout 有值但未超时的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_operation_timeout(Some(Duration::from_millis(200))) // operation_timeout 有值
        .build();

    let result = executor
        .run_async(|| async {
            // 操作时间短于超时时间
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[tokio::test]
async fn test_execute_operation_async_with_some_timeout_exceeded() {
    // 测试异步操作 operation_timeout 有值且超时的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_operation_timeout(Some(Duration::from_millis(50))) // operation_timeout 有值且很短
        .set_delay_strategy(RetryDelayStrategy::None)
        .build();

    let result = executor
        .run_async(|| async {
            // 操作时间长于超时时间
            tokio::time::sleep(Duration::from_millis(200)).await;
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        })
        .await;

    // 应该失败，因为每次操作都超时
    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { .. }) => {
            // 预期的错误类型
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }
}

// ==================== run() 测试 ====================

#[test]
fn test_run_with_check_max_duration_returns_none() {
    // 测试 run() 中 check_max_duration_exceeded 返回 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_max_duration(None) // max_duration 为 None，check_max_duration_exceeded 返回 None
        .build();

    let result = executor.run(|| Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string()));

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[test]
fn test_run_with_check_max_duration_returns_some() {
    // 测试 run() 中 check_max_duration_exceeded 返回 Some(error) 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(100)
        .set_max_duration(Some(Duration::from_millis(100))) // max_duration 很短，会超时
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(50),
        })
        .build();

    let result = executor.run(|| {
        std::thread::sleep(Duration::from_millis(10));
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxDurationExceeded { .. }) => {
            // 预期的错误类型，check_max_duration_exceeded 返回了 Some(error)
        }
        _ => panic!("Expected MaxDurationExceeded error"),
    }
}

#[test]
fn test_run_with_max_duration_not_exceeded_but_max_attempts_exceeded() {
    // 测试 max_duration 未超时但达到最大重试次数的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_max_duration(Some(Duration::from_secs(10))) // max_duration 很长，不会超时
        .set_delay_strategy(RetryDelayStrategy::None)
        .build();

    let result = executor.run(|| {
        Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
    });

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { attempts, .. }) => {
            assert_eq!(attempts, 3);
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }
}

// ==================== run_async() 测试 ====================

#[tokio::test]
async fn test_run_async_with_check_max_duration_returns_none() {
    // 测试 run_async() 中 check_max_duration_exceeded 返回 None 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_max_duration(None) // max_duration 为 None，check_max_duration_exceeded 返回 None
        .build();

    let result = executor
        .run_async(|| async { Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string()) })
        .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
}

#[tokio::test]
async fn test_run_async_with_check_max_duration_returns_some() {
    // 测试 run_async() 中 check_max_duration_exceeded 返回 Some(error) 的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(100)
        .set_max_duration(Some(Duration::from_millis(100))) // max_duration 很短，会超时
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(50),
        })
        .build();

    let result = executor
        .run_async(|| async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
        })
        .await;

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxDurationExceeded { .. }) => {
            // 预期的错误类型，check_max_duration_exceeded 返回了 Some(error)
        }
        _ => panic!("Expected MaxDurationExceeded error"),
    }
}

#[tokio::test]
async fn test_run_async_with_max_duration_not_exceeded_but_max_attempts_exceeded() {
    // 测试异步版本 max_duration 未超时但达到最大重试次数的情况
    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_max_duration(Some(Duration::from_secs(10))) // max_duration 很长，不会超时
        .set_delay_strategy(RetryDelayStrategy::None)
        .build();

    let result = executor
        .run_async(|| async {
            Err(Box::new(TestError("Always fail".to_string())) as Box<dyn Error + Send + Sync>)
        })
        .await;

    assert!(result.is_err());
    match result {
        Err(RetryError::MaxAttemptsExceeded { attempts, .. }) => {
            assert_eq!(attempts, 3);
        }
        _ => panic!("Expected MaxAttemptsExceeded error"),
    }
}

// ==================== 综合测试 ====================

#[test]
fn test_all_listeners_triggered_in_sequence() {
    // 测试所有监听器按顺序触发的情况
    let retry_count = Arc::new(Mutex::new(0));
    let retry_count_clone = retry_count.clone();
    let success_count = Arc::new(Mutex::new(0));
    let success_count_clone = success_count.clone();
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(10),
        })
        .on_retry(move |_event: &RetryEvent<String>| {
            *retry_count_clone.lock().unwrap() += 1;
        })
        .on_success(move |_event: &SuccessEvent<String>| {
            *success_count_clone.lock().unwrap() += 1;
        })
        .build();

    let result = executor.run(|| {
        let mut count = attempt_count_clone.lock().unwrap();
        *count += 1;
        let current = *count;
        drop(count);

        if current < 2 {
            Err(Box::new(TestError("Temporary failure".to_string()))
                as Box<dyn Error + Send + Sync>)
        } else {
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
    assert_eq!(*retry_count.lock().unwrap(), 1); // 第一次失败后触发重试
    assert_eq!(*success_count.lock().unwrap(), 1); // 第二次成功后触发成功监听器
}

#[test]
fn test_no_listeners_all_branches() {
    // 测试没有任何监听器的情况下，所有分支都能正常工作
    let attempt_count = Arc::new(Mutex::new(0));
    let attempt_count_clone = attempt_count.clone();

    let executor = RetryBuilder::<String>::new()
        .set_max_attempts(3)
        .set_delay_strategy(RetryDelayStrategy::Fixed {
            delay: Duration::from_millis(10),
        })
        // 没有设置任何监听器
        .build();

    let result = executor.run(|| {
        let mut count = attempt_count_clone.lock().unwrap();
        *count += 1;
        let current = *count;
        drop(count);

        if current < 2 {
            Err(Box::new(TestError("Temporary failure".to_string()))
                as Box<dyn Error + Send + Sync>)
        } else {
            Ok::<String, Box<dyn Error + Send + Sync>>("SUCCESS".to_string())
        }
    });

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "SUCCESS");
    assert_eq!(*attempt_count.lock().unwrap(), 2);
}
