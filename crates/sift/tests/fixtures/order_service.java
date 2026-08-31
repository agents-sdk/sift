package com.example.orders.service;

import com.example.orders.model.Order;
import com.example.orders.model.OrderStatus;
import com.example.orders.repo.OrderRepository;
import com.example.orders.exception.OrderNotFoundException;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;
import java.math.BigDecimal;
import java.time.Instant;
import java.util.List;
import java.util.stream.Collectors;

/** 订单服务:负责订单生命周期管理与查询。 */
@Service
public class OrderService {

    private final OrderRepository orderRepository;
    private final PaymentGateway paymentGateway;
    private final NotificationClient notificationClient;

    public OrderService(OrderRepository orderRepository,
                        PaymentGateway paymentGateway,
                        NotificationClient notificationClient) {
        this.orderRepository = orderRepository;
        this.paymentGateway = paymentGateway;
        this.notificationClient = notificationClient;
    }

    @Transactional
    public Order createOrder(CreateOrderRequest request) {
        validate(request);
        Order order = new Order();
        order.setUserId(request.getUserId());
        order.setItems(request.getItems());
        order.setAmount(request.getItems().stream()
                .map(i -> i.getPrice().multiply(BigDecimal.valueOf(i.getQuantity())))
                .reduce(BigDecimal.ZERO, BigDecimal::add));
        order.setStatus(OrderStatus.PENDING);
        order.setCreatedAt(Instant.now());
        Order saved = orderRepository.save(order);
        try {
            PaymentResult payment = paymentGateway.charge(saved.getAmount(), saved.getUserId());
            if (!payment.isSuccess()) {
                saved.setStatus(OrderStatus.PAYMENT_FAILED);
                orderRepository.save(saved);
                notificationClient.notify(saved.getUserId(), "支付失败,订单已保留");
                return saved;
            }
            saved.setStatus(OrderStatus.PAID);
            saved.setPaymentId(payment.getPaymentId());
            orderRepository.save(saved);
        } catch (PaymentGatewayException e) {
            saved.setStatus(OrderStatus.PAYMENT_ERROR);
            orderRepository.save(saved);
            throw e;
        }
        notificationClient.notify(saved.getUserId(), "下单成功");
        auditLog.record("order.created", saved.getId());
        metrics.increment("orders.created");
        return saved;
    }

    @Transactional(readOnly = true)
    public Order findById(Long id) {
        return orderRepository.findById(id)
                .orElseThrow(() -> new OrderNotFoundException(id));
    }

    @Transactional(readOnly = true)
    public List<OrderSummary> listByUser(Long userId, int page, int size) {
        return orderRepository.findByUserIdOrderByCreatedAtDesc(userId)
                .stream()
                .skip((long) page * size)
                .limit(size)
                .map(OrderSummary::from)
                .collect(Collectors.toList());
    }

    @Transactional
    public void cancelOrder(Long id, String reason) {
        Order order = findById(id);
        if (order.getStatus() == OrderStatus.SHIPPED) {
            throw new IllegalStateException("已发货订单不可取消");
        }
        if (order.getStatus() == OrderStatus.CANCELLED) {
            return;
        }
        order.setStatus(OrderStatus.CANCELLED);
        order.setCancelReason(reason);
        order.setCancelledAt(Instant.now());
        orderRepository.save(order);
        if (order.getPaymentId() != null) {
            paymentGateway.refund(order.getPaymentId(), order.getAmount());
        }
        notificationClient.notify(order.getUserId(), "订单已取消: " + reason);
        auditLog.record("order.cancelled", id);
    }

    @Transactional
    public Order shipOrder(Long id, String trackingNo) {
        Order order = findById(id);
        if (order.getStatus() != OrderStatus.PAID) {
            throw new IllegalStateException("仅已支付订单可发货");
        }
        order.setStatus(OrderStatus.SHIPPED);
        order.setTrackingNo(trackingNo);
        order.setShippedAt(Instant.now());
        Order saved = orderRepository.save(order);
        notificationClient.notify(saved.getUserId(), "已发货,单号 " + trackingNo);
        metrics.increment("orders.shipped");
        return saved;
    }

    private void validate(CreateOrderRequest request) {
        if (request.getUserId() == null) {
            throw new IllegalArgumentException("userId 不能为空");
        }
        if (request.getItems() == null || request.getItems().isEmpty()) {
            throw new IllegalArgumentException("订单项不能为空");
        }
        for (Item item : request.getItems()) {
            if (item.getQuantity() <= 0) {
                throw new IllegalArgumentException("数量必须为正数");
            }
        }
    }
}
